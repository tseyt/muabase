#![allow(non_snake_case)]

pub mod domain;
mod tables;
#[cfg(test)]
mod tests;

pub use tables::tables as muamalat_tables;

#[psibase::service(name = "muamalat", tables = "tables::tables")]
pub mod service {
    pub use crate::domain::{
        apply_milestone_release, apply_raise_dispute, apply_tahkim_vote, build_validated_contract,
        parse_ruling, validate_contract_kind, STATUS_CREATED, STATUS_FUNDED,
    };
    use crate::tables::tables::*;
    use async_graphql::Object;
    use muabase_institutional::ReviewerAttestation;
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
        add_index("templateRegistered", 0);
        add_index("contractCreated", 0);
        add_index("milestoneReleased", 0);
        add_index("disputeRaised", 0);
        add_index("tahkimAwardSettled", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("muamalat service not initialized");
    }

    /// Registers a scholar-attested commercial contract template (`ISTISNA` or `MUDARABAH`).
    #[action]
    pub fn registerTemplate(
        contract_kind: String,
        template_hash: Checksum256,
        certificate_hash: Checksum256,
        standard_ref: String,
    ) -> u64 {
        validate_contract_kind(&contract_kind);
        let scholar = get_sender();
        let att = ReviewerAttestation {
            reviewer_account: scholar.to_string(),
            template_hash: template_hash.0,
            certificate_hash: certificate_hash.0,
            standard_ref: standard_ref.clone(),
            revoked: false,
        };
        att.validate_active(&template_hash.0)
            .expect("Invalid scholar template attestation");

        let template_id = SeqIdRow::next_template();
        let row = ContractTemplate {
            template_id,
            contract_kind,
            scholar_account: scholar,
            template_hash: template_hash.clone(),
            certificate_hash,
            standard_ref,
            active: true,
        };
        TemplateTable::new().put(&row).unwrap();
        Wrapper::emit()
            .history()
            .templateRegistered(template_id, scholar, template_hash);
        template_id
    }

    /// Creates an escrowed commercial contract pinned to a scholar-attested template hash
    /// and a 3-arbitrator *tahkim* panel.
    #[action]
    pub fn createContract(
        template_id: u64,
        expected_template_hash: Checksum256,
        contractor: AccountNumber,
        arbitrators: Vec<AccountNumber>,
        milestone_amounts: Vec<u64>,
        terms_commitment_hash: Checksum256,
    ) -> u64 {
        let funder = get_sender();
        let tpl = TemplateTable::read()
            .get_index_pk()
            .get(&template_id)
            .expect("ContractTemplate DNE");
        let contract_id = SeqIdRow::next_contract();
        let row = build_validated_contract(
            contract_id,
            &tpl,
            &expected_template_hash,
            funder,
            contractor,
            arbitrators,
            milestone_amounts,
            terms_commitment_hash,
        );
        ContractTable::new().put(&row).unwrap();
        Wrapper::emit()
            .history()
            .contractCreated(contract_id, funder, contractor, row.total_escrow);
        contract_id
    }

    /// Marks the contract escrow funded (`CREATED` -> `FUNDED`).
    #[action]
    pub fn fundEscrow(contract_id: u64) {
        let mut contract = ContractTable::read()
            .get_index_pk()
            .get(&contract_id)
            .expect("MuamalatContract DNE");
        assert_eq!(get_sender(), contract.funder, "Only funder may fund escrow");
        assert_eq!(
            contract.status, STATUS_CREATED,
            "Contract is not in {STATUS_CREATED} state"
        );
        contract.status = STATUS_FUNDED.to_string();
        ContractTable::new().put(&contract).unwrap();
    }

    /// Happy-path milestone escrow release by funder (strictly blocked while `DISPUTED`).
    #[action]
    pub fn releaseMilestone(
        contract_id: u64,
        milestone_evidence_hash: Checksum256,
    ) -> Checksum256 {
        let mut contract = ContractTable::read()
            .get_index_pk()
            .get(&contract_id)
            .expect("MuamalatContract DNE");
        let leaf = apply_milestone_release(&mut contract, get_sender(), &milestone_evidence_hash);
        let idx = contract.milestones_released;
        let amount = contract.milestone_amounts[(idx - 1) as usize];
        ContractTable::new().put(&contract).unwrap();
        Wrapper::emit()
            .history()
            .milestoneReleased(contract_id, idx, amount, leaf.clone());
        leaf
    }

    /// Freezes a `FUNDED` contract into `DISPUTED` state for 2-of-3 *tahkim* arbitration.
    #[action]
    pub fn raiseDispute(contract_id: u64, evidence_commitment_hash: Checksum256) {
        let sender = get_sender();
        let mut contract = ContractTable::read()
            .get_index_pk()
            .get(&contract_id)
            .expect("MuamalatContract DNE");
        let dispute = apply_raise_dispute(&mut contract, sender, evidence_commitment_hash.clone());
        ContractTable::new().put(&contract).unwrap();
        DisputeTable::new().put(&dispute).unwrap();
        Wrapper::emit()
            .history()
            .disputeRaised(contract_id, sender, evidence_commitment_hash);
    }

    /// Casts a *tahkim* arbitrator ruling vote; settles remaining frozen escrow on 2-of-3 quorum.
    #[action]
    pub fn castTahkimVote(
        contract_id: u64,
        ruling_kind: String,
        funder_amount: u64,
        contractor_amount: u64,
    ) -> Option<Checksum256> {
        let sender = get_sender();
        let mut contract = ContractTable::read()
            .get_index_pk()
            .get(&contract_id)
            .expect("MuamalatContract DNE");
        let mut dispute = DisputeTable::read()
            .get_index_pk()
            .get(&contract_id)
            .expect("TahkimDispute DNE");

        let ruling = parse_ruling(&ruling_kind, funder_amount, contractor_amount);
        let award_leaf = apply_tahkim_vote(&mut contract, &mut dispute, sender, ruling);
        ContractTable::new().put(&contract).unwrap();
        DisputeTable::new().put(&dispute).unwrap();
        if let Some(leaf) = award_leaf.clone() {
            Wrapper::emit().history().tahkimAwardSettled(
                contract_id,
                dispute.final_ruling.clone().unwrap_or_default(),
                leaf,
            );
        }
        award_leaf
    }

    #[event(history)]
    pub fn templateRegistered(
        template_id: u64,
        scholar: AccountNumber,
        template_hash: Checksum256,
    ) {
    }

    #[event(history)]
    pub fn contractCreated(
        contract_id: u64,
        funder: AccountNumber,
        contractor: AccountNumber,
        total_escrow: u64,
    ) {
    }

    #[event(history)]
    pub fn milestoneReleased(
        contract_id: u64,
        milestone_index: u32,
        amount: u64,
        receipt_leaf_hash: Checksum256,
    ) {
    }

    #[event(history)]
    pub fn disputeRaised(
        contract_id: u64,
        raised_by: AccountNumber,
        evidence_commitment_hash: Checksum256,
    ) {
    }

    #[event(history)]
    pub fn tahkimAwardSettled(
        contract_id: u64,
        ruling: String,
        award_receipt_leaf_hash: Checksum256,
    ) {
    }

    struct Query;

    #[Object]
    impl Query {
        async fn template(&self, template_id: u64) -> Option<ContractTemplate> {
            TemplateTable::read().get_index_pk().get(&template_id)
        }

        async fn templates(&self) -> Vec<ContractTemplate> {
            TemplateTable::read().get_index_pk().iter().collect()
        }

        async fn contract(&self, contract_id: u64) -> Option<MuamalatContract> {
            ContractTable::read().get_index_pk().get(&contract_id)
        }

        async fn contracts(&self) -> Vec<MuamalatContract> {
            ContractTable::read().get_index_pk().iter().collect()
        }

        async fn dispute(&self, contract_id: u64) -> Option<TahkimDispute> {
            DisputeTable::read().get_index_pk().get(&contract_id)
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
