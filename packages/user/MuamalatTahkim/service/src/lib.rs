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
        pub next_template_id: u64,
        pub next_contract_id: u64,
    }
    impl SeqIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_template() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_template_id += 1;
            table.put(&row).unwrap();
            row.next_template_id
        }

        pub fn next_contract() -> u64 {
            let table = SeqIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_contract_id += 1;
            table.put(&row).unwrap();
            row.next_contract_id
        }
    }

    /// Scholar-attested commercial contract template (`ISTISNA` or `MUDARABAH`).
    #[table(name = "TemplateTable", index = 2)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct ContractTemplate {
        pub template_id: u64,
        pub contract_kind: String, // "ISTISNA" | "MUDARABAH"
        pub scholar_account: AccountNumber,
        pub template_hash: Checksum256,
        pub certificate_hash: Checksum256,
        pub standard_ref: String, // e.g. "AAOIFI-11" or "AAOIFI-13"
        pub active: bool,
    }
    impl ContractTemplate {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.template_id
        }
    }

    /// Deterministic escrowed commercial Muamalat contract state machine.
    #[table(name = "ContractTable", index = 3)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct MuamalatContract {
        pub contract_id: u64,
        pub template_id: u64,
        pub template_hash: Checksum256,
        pub contract_kind: String,
        pub funder: AccountNumber,
        pub contractor: AccountNumber,
        pub arbitrators: Vec<AccountNumber>, // Exact 3-member Tahkim panel
        pub total_escrow: u64,
        pub released_amount: u64,
        pub refunded_amount: u64,
        pub milestone_amounts: Vec<u64>,
        pub milestones_released: u32,
        pub terms_commitment_hash: Checksum256, // Zero-plaintext B2B terms commitment
        pub status: String, // "CREATED" | "FUNDED" | "DISPUTED" | "SETTLED"
        pub last_receipt_leaf_hash: Option<Checksum256>,
    }
    impl MuamalatContract {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.contract_id
        }
    }

    /// Active or resolved 2-of-3 Tahkim arbitration dispute record.
    #[table(name = "DisputeTable", index = 4)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct TahkimDispute {
        pub contract_id: u64,
        pub raised_by: AccountNumber,
        pub evidence_commitment_hash: Checksum256,
        pub votes_json: Vec<String>,
        pub resolved: bool,
        pub final_ruling: Option<String>,
        pub award_receipt_leaf_hash: Option<Checksum256>,
    }
    impl TahkimDispute {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.contract_id
        }
    }
}

#[psibase::service(name = "muamalat", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use muabase_institutional::{
        JournalEntry, JournalLine, ReviewerAttestation, TahkimPanel, TahkimRuling, TahkimVote,
    };
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

    /// Registers a scholar-attested commercial contract template (`ISTISNA` or `MUDARABAH`).
    #[action]
    pub fn registerTemplate(
        contract_kind: String,
        template_hash: Checksum256,
        certificate_hash: Checksum256,
        standard_ref: String,
    ) -> u64 {
        assert!(
            contract_kind == "ISTISNA" || contract_kind == "MUDARABAH",
            "contract_kind must be ISTISNA or MUDARABAH"
        );
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
        assert_eq!(contract.status, "CREATED", "Contract is not in CREATED state");
        contract.status = "FUNDED".to_string();
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
        ruling_kind: String, // "AWARD_CONTRACTOR" | "REFUND_FUNDER" | "SPLIT"
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

    pub fn parse_ruling(
        ruling_kind: &str,
        funder_amount: u64,
        contractor_amount: u64,
    ) -> TahkimRuling {
        match ruling_kind {
            "AWARD_CONTRACTOR" => TahkimRuling::AwardCounterparty,
            "REFUND_FUNDER" => TahkimRuling::RefundFunder,
            "SPLIT" => TahkimRuling::Split {
                funder_amount,
                counterparty_amount: contractor_amount,
            },
            _ => panic!("ruling_kind must be AWARD_CONTRACTOR, REFUND_FUNDER, or SPLIT"),
        }
    }

    pub fn build_validated_contract(
        contract_id: u64,
        tpl: &ContractTemplate,
        expected_template_hash: &Checksum256,
        funder: AccountNumber,
        contractor: AccountNumber,
        arbitrators: Vec<AccountNumber>,
        milestone_amounts: Vec<u64>,
        terms_commitment_hash: Checksum256,
    ) -> MuamalatContract {
        let att = ReviewerAttestation {
            reviewer_account: tpl.scholar_account.to_string(),
            template_hash: tpl.template_hash.0,
            certificate_hash: tpl.certificate_hash.0,
            standard_ref: tpl.standard_ref.clone(),
            revoked: !tpl.active,
        };
        att.validate_active(&expected_template_hash.0)
            .expect("Scholar template attestation verification failed");

        assert!(funder != contractor, "Funder and contractor must be distinct");
        assert!(
            terms_commitment_hash != Checksum256::from([0u8; 32]),
            "PII Guard: terms_commitment_hash must be a non-zero 32-byte SHA-256 digest"
        );
        assert!(
            arbitrators.len() == 3,
            "Tahkim panel requires exactly 3 distinct arbitrators"
        );
        let _panel = TahkimPanel::new([
            arbitrators[0].to_string(),
            arbitrators[1].to_string(),
            arbitrators[2].to_string(),
        ])
        .expect("Invalid tahkim arbitrator panel");
        assert!(
            !arbitrators.contains(&funder) && !arbitrators.contains(&contractor),
            "Arbitrators must be independent of funder and contractor"
        );
        assert!(
            !milestone_amounts.is_empty() && milestone_amounts.iter().all(|&m| m > 0),
            "All milestone_amounts must be strictly positive"
        );
        let total_escrow: u64 = milestone_amounts.iter().sum();

        MuamalatContract {
            contract_id,
            template_id: tpl.template_id,
            template_hash: tpl.template_hash.clone(),
            contract_kind: tpl.contract_kind.clone(),
            funder,
            contractor,
            arbitrators,
            total_escrow,
            released_amount: 0,
            refunded_amount: 0,
            milestone_amounts,
            milestones_released: 0,
            terms_commitment_hash,
            status: "CREATED".to_string(),
            last_receipt_leaf_hash: None,
        }
    }

    pub fn apply_milestone_release(
        contract: &mut MuamalatContract,
        sender: AccountNumber,
        milestone_evidence_hash: &Checksum256,
    ) -> Checksum256 {
        assert_eq!(sender, contract.funder, "Only funder may release a milestone");
        assert_eq!(
            contract.status, "FUNDED",
            "Milestone release blocked: contract status is '{}' (must be FUNDED)",
            contract.status
        );
        assert!(
            *milestone_evidence_hash != Checksum256::from([0u8; 32]),
            "milestone_evidence_hash must be non-zero"
        );
        let idx = contract.milestones_released as usize;
        assert!(
            idx < contract.milestone_amounts.len(),
            "All milestones already released"
        );
        let amount = contract.milestone_amounts[idx];

        let entry = JournalEntry {
            entry_id: contract.contract_id * 1000 + (idx as u64 + 1),
            category: format!("{}:milestone_{}", contract.contract_kind, idx + 1),
            lines: vec![
                JournalLine {
                    account: format!("escrow:{}", contract.contract_id),
                    delta: -(amount as i64),
                },
                JournalLine {
                    account: format!("contractor:{}", contract.contractor),
                    delta: amount as i64,
                },
            ],
            commitment_hash: milestone_evidence_hash.0,
        };
        let leaf = Checksum256::from(
            entry
                .receipt_leaf_hash("receipt/muamalat")
                .expect("Milestone double-entry conservation violated"),
        );

        contract.released_amount += amount;
        contract.milestones_released += 1;
        contract.last_receipt_leaf_hash = Some(leaf.clone());
        if (contract.milestones_released as usize) == contract.milestone_amounts.len() {
            contract.status = "SETTLED".to_string();
        }
        leaf
    }

    pub fn apply_raise_dispute(
        contract: &mut MuamalatContract,
        sender: AccountNumber,
        evidence_commitment_hash: Checksum256,
    ) -> TahkimDispute {
        assert!(
            sender == contract.funder || sender == contract.contractor,
            "Only funder or contractor may raise a dispute"
        );
        assert_eq!(
            contract.status, "FUNDED",
            "Disputes may only be raised on a FUNDED contract"
        );
        assert!(
            evidence_commitment_hash != Checksum256::from([0u8; 32]),
            "evidence_commitment_hash must be non-zero"
        );
        contract.status = "DISPUTED".to_string();
        TahkimDispute {
            contract_id: contract.contract_id,
            raised_by: sender,
            evidence_commitment_hash,
            votes_json: vec![],
            resolved: false,
            final_ruling: None,
            award_receipt_leaf_hash: None,
        }
    }

    pub fn apply_tahkim_vote(
        contract: &mut MuamalatContract,
        dispute: &mut TahkimDispute,
        arbitrator: AccountNumber,
        ruling: TahkimRuling,
    ) -> Option<Checksum256> {
        assert_eq!(contract.status, "DISPUTED", "Contract is not in DISPUTED state");
        assert!(!dispute.resolved, "Dispute is already resolved");

        let remaining = contract
            .total_escrow
            .checked_sub(contract.released_amount)
            .expect("released_amount exceeds total_escrow");

        let mut panel = TahkimPanel::new([
            contract.arbitrators[0].to_string(),
            contract.arbitrators[1].to_string(),
            contract.arbitrators[2].to_string(),
        ])
        .expect("Corrupt panel");
        for raw in &dispute.votes_json {
            let v: TahkimVote = serde_json::from_str(raw).expect("Corrupt stored TahkimVote");
            panel.votes.push(v);
        }

        let resolved = panel
            .cast_vote(&arbitrator.to_string(), ruling, remaining)
            .expect("Tahkim vote rejected");

        dispute.votes_json = panel
            .votes
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect();

        if let Some(final_ruling) = resolved {
            let (funder_back, contractor_pay) = match final_ruling {
                TahkimRuling::AwardCounterparty => (0, remaining),
                TahkimRuling::RefundFunder => (remaining, 0),
                TahkimRuling::Split {
                    funder_amount,
                    counterparty_amount,
                } => (funder_amount, counterparty_amount),
            };
            assert_eq!(
                funder_back + contractor_pay,
                remaining,
                "Tahkim award must conserve remaining escrow"
            );

            let mut lines = vec![JournalLine {
                account: format!("escrow:{}", contract.contract_id),
                delta: -(remaining as i64),
            }];
            if funder_back > 0 {
                lines.push(JournalLine {
                    account: format!("funder:{}", contract.funder),
                    delta: funder_back as i64,
                });
            }
            if contractor_pay > 0 {
                lines.push(JournalLine {
                    account: format!("contractor:{}", contract.contractor),
                    delta: contractor_pay as i64,
                });
            }

            let entry = JournalEntry {
                entry_id: contract.contract_id * 1000 + 999,
                category: "tahkim_award".to_string(),
                lines,
                commitment_hash: dispute.evidence_commitment_hash.0,
            };
            let leaf = Checksum256::from(
                entry
                    .receipt_leaf_hash("receipt/tahkim")
                    .expect("Tahkim award conservation violated"),
            );

            contract.released_amount += contractor_pay;
            contract.refunded_amount += funder_back;
            contract.status = "SETTLED".to_string();
            contract.last_receipt_leaf_hash = Some(leaf.clone());
            dispute.resolved = true;
            dispute.final_ruling = Some(serde_json::to_string(&final_ruling).unwrap());
            dispute.award_receipt_leaf_hash = Some(leaf.clone());
            Some(leaf)
        } else {
            None
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
        apply_milestone_release, apply_raise_dispute, apply_tahkim_vote, build_validated_contract,
        parse_ruling,
    };
    use crate::tables::ContractTemplate;
    use psibase::{account, Checksum256};

    fn sample_template() -> ContractTemplate {
        ContractTemplate {
            template_id: 1,
            contract_kind: "ISTISNA".into(),
            scholar_account: account!("scholar"),
            template_hash: Checksum256::from([0xaa; 32]),
            certificate_hash: Checksum256::from([0xbb; 32]),
            standard_ref: "AAOIFI-11".into(),
            active: true,
        }
    }

    #[test]
    fn given_scholar_attested_istisna_when_milestones_released_then_escrow_settles_with_receipt() {
        let tpl = sample_template();
        let mut c = build_validated_contract(
            1,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![40_000, 60_000],
            Checksum256::from([0xcc; 32]),
        );
        c.status = "FUNDED".into();

        let r1 = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x01; 32]));
        assert_ne!(r1, Checksum256::from([0u8; 32]));
        assert_eq!(c.released_amount, 40_000);
        assert_eq!(c.status, "FUNDED");

        let r2 = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x02; 32]));
        assert_ne!(r2, r1);
        assert_eq!(c.released_amount, 100_000);
        assert_eq!(c.status, "SETTLED");
    }

    #[test]
    fn given_disputed_contract_when_two_of_three_tahkim_vote_split_then_remaining_escrow_conserves_and_settles() {
        let tpl = sample_template();
        let mut c = build_validated_contract(
            2,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![40_000, 60_000],
            Checksum256::from([0xcc; 32]),
        );
        c.status = "FUNDED".into();
        let _ = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x01; 32]));

        // Freeze remaining 60,000 in dispute
        let mut d = apply_raise_dispute(&mut c, account!("builder"), Checksum256::from([0xdd; 32]));
        assert_eq!(c.status, "DISPUTED");

        let ruling = parse_ruling("SPLIT", 20_000, 40_000);
        assert!(apply_tahkim_vote(&mut c, &mut d, account!("arb1"), ruling.clone()).is_none());
        let award = apply_tahkim_vote(&mut c, &mut d, account!("arb2"), ruling);
        assert!(award.is_some());
        assert!(d.resolved);
        assert_eq!(c.status, "SETTLED");
        assert_eq!(c.released_amount, 80_000); // 40k milestone + 40k award
        assert_eq!(c.refunded_amount, 20_000);
        assert_eq!(c.released_amount + c.refunded_amount, c.total_escrow);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Milestone release blocked")]
    fn given_disputed_contract_when_funder_attempts_unilateral_release_then_blocked() {
        let tpl = sample_template();
        let mut c = build_validated_contract(
            3,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![50_000],
            Checksum256::from([0xcc; 32]),
        );
        c.status = "FUNDED".into();
        let _ = apply_raise_dispute(&mut c, account!("buyer"), Checksum256::from([0xdd; 32]));
        let _ = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x01; 32]));
    }
}
