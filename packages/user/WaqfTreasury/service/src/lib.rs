#![allow(non_snake_case)]

pub mod domain;
mod tables;
#[cfg(test)]
mod tests;

pub use tables::tables as waqf_tables;

#[psibase::service(name = "waqf-treas", tables = "tables::tables")]
pub mod service {
    pub use crate::domain::{
        apply_disbursement_approval, apply_key_recovery_approval, settle_disbursement,
        validate_disbursement_rules, validate_endowment_params, validate_key_recovery_proposal,
    };
    use crate::tables::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::*;

    #[action]
    fn init() {
        let table = InitTable::new();
        if table.get_index_pk().get(&()).is_none() {
            table.put(&InitRow {}).unwrap();
        }
        let add_index = |method: &str, col: u8| {
            events::Wrapper::call().addIndex(
                EventDb::HistoryEvent,
                Wrapper::SERVICE,
                MethodNumber::from(method),
                col,
            );
        };
        add_index("endowmentCreated", 0);
        add_index("yieldRecorded", 0);
        add_index("disbursementSettled", 0);
        add_index("boardKeyRotated", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("waqf-treas service not initialized");
    }

    /// Creates a rule-locked Waqf / Masjid endowment whose `corpus_principal` is immutably preserved.
    #[action]
    pub fn createEndowment(
        label: String,
        corpus_principal: u64,
        shurut_hash: Checksum256,
        scholar_attestation_hash: Checksum256,
        allowed_categories: Vec<String>,
        max_single_disbursement: u64,
        board_members: Vec<AccountNumber>,
        quorum_threshold: u32,
    ) -> u64 {
        validate_endowment_params(
            &label,
            corpus_principal,
            &shurut_hash,
            &scholar_attestation_hash,
            &allowed_categories,
            &board_members,
            quorum_threshold,
        );
        let endowment_id = SeqIdRow::next_endowment();
        let row = Endowment {
            endowment_id,
            founder: get_sender(),
            label,
            corpus_principal,
            yield_balance: 0,
            total_disbursed: 0,
            shurut_hash,
            scholar_attestation_hash,
            allowed_categories,
            max_single_disbursement,
            board_members,
            quorum_threshold,
            principal_locked: true,
        };
        EndowmentTable::new().put(&row).unwrap();
        Wrapper::emit()
            .history()
            .endowmentCreated(endowment_id, row.founder, corpus_principal);
        endowment_id
    }

    /// Credits distributable yield into an endowment without altering its locked corpus principal.
    #[action]
    pub fn recordYieldInflow(endowment_id: u64, amount: u64, source_ref: String) {
        assert!(amount > 0, "Yield inflow must be strictly positive");
        assert!(!source_ref.trim().is_empty(), "source_ref must be non-empty");
        let mut endowment = EndowmentTable::read()
            .get_index_pk()
            .get(&endowment_id)
            .expect("Endowment DNE");
        let sender = get_sender();
        assert!(
            sender == endowment.founder || endowment.board_members.contains(&sender),
            "Only founder or board member may record yield inflow"
        );
        endowment.yield_balance = endowment
            .yield_balance
            .checked_add(amount)
            .expect("yield_balance overflow");
        EndowmentTable::new().put(&endowment).unwrap();
        Wrapper::emit()
            .history()
            .yieldRecorded(endowment_id, amount, source_ref);
    }

    /// Proposes a yield disbursement subject to immutable *shurut* categories, spending cap, and corpus lock.
    #[action]
    pub fn proposeDisbursement(
        endowment_id: u64,
        recipient: AccountNumber,
        category: String,
        amount: u64,
        evidence_commitment_hash: Checksum256,
    ) -> u64 {
        let sender = get_sender();
        let endowment = EndowmentTable::read()
            .get_index_pk()
            .get(&endowment_id)
            .expect("Endowment DNE");
        assert!(
            endowment.board_members.contains(&sender),
            "Only an active board member may propose a disbursement"
        );
        validate_disbursement_rules(
            &endowment,
            recipient,
            &category,
            amount,
            &evidence_commitment_hash,
        );

        let proposal_id = SeqIdRow::next_proposal();
        let prop = DisbursementProposal {
            proposal_id,
            endowment_id,
            proposer: sender,
            recipient,
            category,
            amount,
            evidence_commitment_hash,
            approvals: vec![sender],
            executed: false,
            receipt_leaf_hash: None,
        };
        DisbursementTable::new().put(&prop).unwrap();
        proposal_id
    }

    /// Approves a pending disbursement; settles automatically once active `approvals >= quorum_threshold`.
    #[action]
    pub fn approveDisbursement(proposal_id: u64) -> Option<Checksum256> {
        let sender = get_sender();
        let mut prop = DisbursementTable::read()
            .get_index_pk()
            .get(&proposal_id)
            .expect("DisbursementProposal DNE");
        let mut endowment = EndowmentTable::read()
            .get_index_pk()
            .get(&prop.endowment_id)
            .expect("Endowment DNE");

        let receipt = apply_disbursement_approval(&mut endowment, &mut prop, sender);
        EndowmentTable::new().put(&endowment).unwrap();
        DisbursementTable::new().put(&prop).unwrap();
        if let Some(leaf) = receipt.clone() {
            Wrapper::emit().history().disbursementSettled(
                proposal_id,
                endowment.endowment_id,
                prop.recipient,
                prop.amount,
                leaf,
            );
        }
        receipt
    }

    /// Direct execution path for `staged-tx` / `auth-dyn` multisig-authorized board accounts.
    #[action]
    pub fn executeStagedDisbursement(
        endowment_id: u64,
        recipient: AccountNumber,
        category: String,
        amount: u64,
        evidence_commitment_hash: Checksum256,
    ) -> Checksum256 {
        let sender = get_sender();
        let mut endowment = EndowmentTable::read()
            .get_index_pk()
            .get(&endowment_id)
            .expect("Endowment DNE");
        assert!(
            sender == endowment.founder,
            "executeStagedDisbursement must be sent by the multisig/staged-tx endowment founder account"
        );
        validate_disbursement_rules(
            &endowment,
            recipient,
            &category,
            amount,
            &evidence_commitment_hash,
        );

        let proposal_id = SeqIdRow::next_proposal();
        let mut prop = DisbursementProposal {
            proposal_id,
            endowment_id,
            proposer: sender,
            recipient,
            category,
            amount,
            evidence_commitment_hash,
            approvals: vec![sender],
            executed: false,
            receipt_leaf_hash: None,
        };
        let leaf = settle_disbursement(&mut endowment, &mut prop);
        EndowmentTable::new().put(&endowment).unwrap();
        DisbursementTable::new().put(&prop).unwrap();
        Wrapper::emit().history().disbursementSettled(
            proposal_id,
            endowment_id,
            recipient,
            amount,
            leaf.clone(),
        );
        leaf
    }

    /// Initiates a board-quorum treasurer key rotation / member recovery proposal.
    #[action]
    pub fn proposeKeyRecovery(
        endowment_id: u64,
        old_member: AccountNumber,
        new_member: AccountNumber,
        new_pubkey_spki_hash: Checksum256,
    ) -> u64 {
        let sender = get_sender();
        let endowment = EndowmentTable::read()
            .get_index_pk()
            .get(&endowment_id)
            .expect("Endowment DNE");
        validate_key_recovery_proposal(
            &endowment,
            sender,
            old_member,
            new_member,
            &new_pubkey_spki_hash,
        );

        let recovery_id = SeqIdRow::next_recovery();
        let rec = KeyRecoveryProposal {
            recovery_id,
            endowment_id,
            old_member,
            new_member,
            new_pubkey_spki_hash,
            approvals: vec![sender],
            executed: false,
        };
        KeyRecoveryTable::new().put(&rec).unwrap();
        recovery_id
    }

    /// Approves a board-quorum key recovery; rotates the seat atomically once quorum is met.
    #[action]
    pub fn approveKeyRecovery(recovery_id: u64) -> bool {
        let sender = get_sender();
        let mut rec = KeyRecoveryTable::read()
            .get_index_pk()
            .get(&recovery_id)
            .expect("KeyRecoveryProposal DNE");
        let mut endowment = EndowmentTable::read()
            .get_index_pk()
            .get(&rec.endowment_id)
            .expect("Endowment DNE");

        let executed = apply_key_recovery_approval(&mut endowment, &mut rec, sender);
        EndowmentTable::new().put(&endowment).unwrap();
        KeyRecoveryTable::new().put(&rec).unwrap();
        if executed {
            Wrapper::emit().history().boardKeyRotated(
                endowment.endowment_id,
                rec.old_member,
                rec.new_member,
                rec.new_pubkey_spki_hash,
            );
        }
        executed
    }

    #[event(history)]
    pub fn endowmentCreated(endowment_id: u64, founder: AccountNumber, corpus_principal: u64) {}

    #[event(history)]
    pub fn yieldRecorded(endowment_id: u64, amount: u64, source_ref: String) {}

    #[event(history)]
    pub fn disbursementSettled(
        proposal_id: u64,
        endowment_id: u64,
        recipient: AccountNumber,
        amount: u64,
        receipt_leaf_hash: Checksum256,
    ) {
    }

    #[event(history)]
    pub fn boardKeyRotated(
        endowment_id: u64,
        old_member: AccountNumber,
        new_member: AccountNumber,
        new_pubkey_spki_hash: Checksum256,
    ) {
    }

    struct Query;

    #[Object]
    impl Query {
        async fn endowment(&self, endowment_id: u64) -> Option<Endowment> {
            EndowmentTable::read().get_index_pk().get(&endowment_id)
        }

        async fn endowments(&self) -> Vec<Endowment> {
            EndowmentTable::read().get_index_pk().iter().collect()
        }

        async fn disbursement(&self, proposal_id: u64) -> Option<DisbursementProposal> {
            DisbursementTable::read().get_index_pk().get(&proposal_id)
        }

        async fn disbursements(&self, endowment_id: Option<u64>) -> Vec<DisbursementProposal> {
            DisbursementTable::read()
                .get_index_pk()
                .iter()
                .filter(|p| endowment_id.map_or(true, |id| p.endowment_id == id))
                .collect()
        }

        async fn key_recovery(&self, recovery_id: u64) -> Option<KeyRecoveryProposal> {
            KeyRecoveryTable::read().get_index_pk().get(&recovery_id)
        }

        async fn aggregate_treasury(&self) -> AggregateTreasury {
            let mut agg = AggregateTreasury::default();
            for e in EndowmentTable::read().get_index_pk().iter() {
                agg.endowment_count = agg.endowment_count.saturating_add(1);
                agg.total_corpus_locked = agg.total_corpus_locked.saturating_add(e.corpus_principal);
                agg.total_yield_available = agg.total_yield_available.saturating_add(e.yield_balance);
                agg.total_disbursed = agg.total_disbursed.saturating_add(e.total_disbursed);
            }
            agg
        }
    }

    #[action]
    #[allow(non_snake_case)]
    fn serveSys(
        request: HttpRequest,
        _socket: Option<i32>,
        _user: Option<AccountNumber>,
    ) -> Option<HttpReply> {
        None.or_else(|| serve_graphql(&request, Query))
    }
}
