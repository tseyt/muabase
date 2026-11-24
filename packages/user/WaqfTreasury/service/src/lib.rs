#![allow(non_snake_case)]

#[psibase::service_tables]
pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::{AccountNumber, Checksum256, Pack, Table, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "SeqIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct SeqIdRow {
        pub next_endowment_id: u64,
        pub next_proposal_id: u64,
        pub next_recovery_id: u64,
    }
    impl SeqIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_endowment() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_endowment_id += 1;
            table.put(&row).unwrap();
            row.next_endowment_id
        }

        pub fn next_proposal() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_proposal_id += 1;
            table.put(&row).unwrap();
            row.next_proposal_id
        }

        pub fn next_recovery() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_recovery_id += 1;
            table.put(&row).unwrap();
            row.next_recovery_id
        }
    }

    /// Rule-locked Waqf / Masjid Endowment with immutable corpus preservation (`Hifz Asl al-Waqf`).
    #[table(name = "EndowmentTable", index = 2)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct Endowment {
        pub endowment_id: u64,
        pub founder: AccountNumber,
        pub label: String,
        pub corpus_principal: u64,
        pub yield_balance: u64,
        pub total_disbursed: u64,
        pub shurut_hash: Checksum256,
        pub scholar_attestation_hash: Checksum256,
        pub allowed_categories: Vec<String>,
        pub max_single_disbursement: u64,
        pub board_members: Vec<AccountNumber>,
        pub quorum_threshold: u32,
        pub principal_locked: bool,
    }
    impl Endowment {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.endowment_id
        }
    }

    /// Board-quorum disbursement proposal & settled Merkle audit receipt.
    #[table(name = "DisbursementTable", index = 3)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct DisbursementProposal {
        pub proposal_id: u64,
        pub endowment_id: u64,
        pub proposer: AccountNumber,
        pub recipient: AccountNumber,
        pub category: String,
        pub amount: u64,
        pub evidence_commitment_hash: Checksum256,
        pub approvals: Vec<AccountNumber>,
        pub executed: bool,
        pub receipt_leaf_hash: Option<Checksum256>,
    }
    impl DisbursementProposal {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.proposal_id
        }
    }

    /// Board-quorum treasurer key rotation / recovery proposal.
    #[table(name = "KeyRecoveryTable", index = 4)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct KeyRecoveryProposal {
        pub recovery_id: u64,
        pub endowment_id: u64,
        pub old_member: AccountNumber,
        pub new_member: AccountNumber,
        pub new_pubkey_spki_hash: Checksum256,
        pub approvals: Vec<AccountNumber>,
        pub executed: bool,
    }
    impl KeyRecoveryProposal {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.recovery_id
        }
    }

    #[derive(Serialize, Deserialize, SimpleObject, Debug, Clone, PartialEq, Eq, Default)]
    pub struct AggregateTreasury {
        pub endowment_count: u64,
        pub total_corpus_locked: u64,
        pub total_yield_available: u64,
        pub total_disbursed: u64,
    }
}

#[psibase::service(name = "waqf-treas", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use muabase_institutional::{CompliancePolicy, JournalEntry, JournalLine};
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
        validate_disbursement_rules(&endowment, recipient, &category, amount, &evidence_commitment_hash);

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

    /// Approves a pending disbursement; settles automatically once `approvals.len() >= quorum_threshold`.
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
        validate_disbursement_rules(&endowment, recipient, &category, amount, &evidence_commitment_hash);

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
        assert!(
            endowment.board_members.contains(&sender) && sender != old_member,
            "Key recovery must be proposed by another active board member"
        );
        assert!(
            endowment.board_members.contains(&old_member),
            "old_member is not on the endowment board"
        );
        assert!(
            !endowment.board_members.contains(&new_member),
            "new_member is already on the endowment board"
        );
        assert!(
            new_pubkey_spki_hash != Checksum256::from([0u8; 32]),
            "new_pubkey_spki_hash must be non-zero"
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
                agg.endowment_count += 1;
                agg.total_corpus_locked += e.corpus_principal;
                agg.total_yield_available += e.yield_balance;
                agg.total_disbursed += e.total_disbursed;
            }
            agg
        }
    }

    pub fn validate_endowment_params(
        label: &str,
        corpus_principal: u64,
        shurut_hash: &Checksum256,
        scholar_attestation_hash: &Checksum256,
        allowed_categories: &[String],
        board_members: &[AccountNumber],
        quorum_threshold: u32,
    ) {
        assert!(!label.trim().is_empty(), "Endowment label must be non-empty");
        assert!(corpus_principal > 0, "Endowment corpus_principal must be positive");
        assert!(
            *shurut_hash != Checksum256::from([0u8; 32]),
            "shurut_hash must be a non-zero 32-byte SHA-256 digest"
        );
        assert!(
            *scholar_attestation_hash != Checksum256::from([0u8; 32]),
            "scholar_attestation_hash must be a non-zero 32-byte SHA-256 digest"
        );
        assert!(
            !allowed_categories.is_empty(),
            "allowed_categories (shurut) must contain at least one category"
        );
        assert!(
            quorum_threshold >= 2 && (quorum_threshold as usize) <= board_members.len(),
            "Board quorum_threshold must be >= 2 and <= board_members.len()"
        );
        for (i, a) in board_members.iter().enumerate() {
            for b in &board_members[i + 1..] {
                assert!(a != b, "Duplicate board member in Waqf board");
            }
        }
    }

    pub fn validate_disbursement_rules(
        endowment: &Endowment,
        recipient: AccountNumber,
        category: &str,
        amount: u64,
        evidence_commitment_hash: &Checksum256,
    ) {
        assert!(endowment.principal_locked, "Waqf corpus lock must be active");
        assert!(
            *evidence_commitment_hash != Checksum256::from([0u8; 32]),
            "PII Guard: evidence_commitment_hash must be a non-zero 32-byte SHA-256 digest"
        );
        assert!(
            amount <= endowment.yield_balance,
            "Waqf Corpus Lock Violation: disbursement ({}) exceeds available yield ({}); principal ({}) cannot be spent",
            amount,
            endowment.yield_balance,
            endowment.corpus_principal
        );

        let policy = CompliancePolicy {
            max_single_disbursement: endowment.max_single_disbursement,
            allowed_categories: endowment.allowed_categories.clone(),
            allowlist_recipients: vec![],
            require_zero_fee_tamlik: true,
        };
        policy
            .validate_disbursement(&recipient.to_string(), category, amount, 0)
            .expect("Shurut policy check failed");
    }

    pub fn settle_disbursement(
        endowment: &mut Endowment,
        prop: &mut DisbursementProposal,
    ) -> Checksum256 {
        validate_disbursement_rules(
            endowment,
            prop.recipient,
            &prop.category,
            prop.amount,
            &prop.evidence_commitment_hash,
        );

        let entry = JournalEntry {
            entry_id: prop.proposal_id,
            category: prop.category.clone(),
            lines: vec![
                JournalLine {
                    account: format!("waqf:{}:yield", endowment.endowment_id),
                    delta: -(prop.amount as i64),
                },
                JournalLine {
                    account: format!("recipient:{}", prop.recipient),
                    delta: prop.amount as i64,
                },
            ],
            commitment_hash: prop.evidence_commitment_hash.0,
        };
        let leaf = Checksum256::from(
            entry
                .receipt_leaf_hash("receipt/waqf")
                .expect("Double-entry Waqf conservation violated"),
        );

        endowment.yield_balance -= prop.amount;
        endowment.total_disbursed += prop.amount;
        prop.executed = true;
        prop.receipt_leaf_hash = Some(leaf.clone());
        leaf
    }

    pub fn apply_disbursement_approval(
        endowment: &mut Endowment,
        prop: &mut DisbursementProposal,
        approver: AccountNumber,
    ) -> Option<Checksum256> {
        assert!(!prop.executed, "DisbursementProposal already executed");
        assert!(
            endowment.board_members.contains(&approver),
            "Only an active board member may approve disbursements"
        );
        assert!(
            !prop.approvals.contains(&approver),
            "Board member has already approved this proposal"
        );
        prop.approvals.push(approver);
        if (prop.approvals.len() as u32) >= endowment.quorum_threshold {
            Some(settle_disbursement(endowment, prop))
        } else {
            None
        }
    }

    pub fn apply_key_recovery_approval(
        endowment: &mut Endowment,
        rec: &mut KeyRecoveryProposal,
        approver: AccountNumber,
    ) -> bool {
        assert!(!rec.executed, "KeyRecoveryProposal already executed");
        assert!(
            endowment.board_members.contains(&approver) && approver != rec.old_member,
            "Only another active board member may approve key recovery"
        );
        assert!(
            !rec.approvals.contains(&approver),
            "Board member has already approved this key recovery"
        );
        rec.approvals.push(approver);
        if (rec.approvals.len() as u32) >= endowment.quorum_threshold {
            let pos = endowment
                .board_members
                .iter()
                .position(|m| *m == rec.old_member)
                .expect("old_member no longer on board");
            endowment.board_members[pos] = rec.new_member;
            rec.executed = true;
            true
        } else {
            false
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

#[cfg(test)]
mod tests {
    use crate::service::{
        apply_disbursement_approval, apply_key_recovery_approval, validate_disbursement_rules,
        validate_endowment_params,
    };
    use crate::tables::{DisbursementProposal, Endowment, KeyRecoveryProposal};
    use psibase::{account, Checksum256};

    fn sample_endowment() -> Endowment {
        let board = vec![account!("chair"), account!("treas"), account!("trustee")];
        let shurut = Checksum256::from([0x11; 32]);
        let scholar = Checksum256::from([0x22; 32]);
        let cats = vec!["scholarship".into(), "masjid_utilities".into()];
        validate_endowment_params("Al-Noor Waqf", 1_000_000, &shurut, &scholar, &cats, &board, 2);
        Endowment {
            endowment_id: 1,
            founder: account!("masjid"),
            label: "Al-Noor Waqf".into(),
            corpus_principal: 1_000_000,
            yield_balance: 50_000,
            total_disbursed: 0,
            shurut_hash: shurut,
            scholar_attestation_hash: scholar,
            allowed_categories: cats,
            max_single_disbursement: 30_000,
            board_members: board,
            quorum_threshold: 2,
            principal_locked: true,
        }
    }

    #[test]
    fn given_shurut_locked_endowment_when_two_of_three_board_approve_yield_disbursement_then_corpus_is_preserved_and_receipt_emitted() {
        let mut e = sample_endowment();
        let mut p = DisbursementProposal {
            proposal_id: 1,
            endowment_id: 1,
            proposer: account!("chair"),
            recipient: account!("school"),
            category: "scholarship".into(),
            amount: 20_000,
            evidence_commitment_hash: Checksum256::from([0x33; 32]),
            approvals: vec![account!("chair")],
            executed: false,
            receipt_leaf_hash: None,
        };

        let receipt = apply_disbursement_approval(&mut e, &mut p, account!("treas"));
        assert!(receipt.is_some());
        assert!(p.executed);
        assert_eq!(e.corpus_principal, 1_000_000);
        assert_eq!(e.yield_balance, 30_000);
        assert_eq!(e.total_disbursed, 20_000);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Waqf Corpus Lock Violation")]
    fn given_shurut_locked_endowment_when_disbursement_exceeds_yield_then_corpus_lock_rejects() {
        let e = sample_endowment();
        validate_disbursement_rules(
            &e,
            account!("school"),
            "scholarship",
            60_000,
            &Checksum256::from([0x33; 32]),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Shurut policy check failed")]
    fn given_shurut_locked_endowment_when_category_is_disallowed_then_rejected() {
        let e = sample_endowment();
        validate_disbursement_rules(
            &e,
            account!("vendor"),
            "unapproved_luxury",
            10_000,
            &Checksum256::from([0x33; 32]),
        );
    }

    #[test]
    fn given_compromised_treasurer_when_board_quorum_approves_key_recovery_then_seat_rotates() {
        let mut e = sample_endowment();
        let mut rec = KeyRecoveryProposal {
            recovery_id: 1,
            endowment_id: 1,
            old_member: account!("treas"),
            new_member: account!("treas2"),
            new_pubkey_spki_hash: Checksum256::from([0x44; 32]),
            approvals: vec![account!("chair")],
            executed: false,
        };
        let done = apply_key_recovery_approval(&mut e, &mut rec, account!("trustee"));
        assert!(done);
        assert!(rec.executed);
        assert!(e.board_members.contains(&account!("treas2")));
        assert!(!e.board_members.contains(&account!("treas")));
    }
}
