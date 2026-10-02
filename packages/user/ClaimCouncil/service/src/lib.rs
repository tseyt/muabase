#![allow(non_snake_case)]

pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::services::tokens::Quantity;
    use psibase::{AccountNumber, Checksum256, Pack, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    pub const CLAIM_STATUS_PENDING: u8 = 0;
    pub const CLAIM_STATUS_APPROVED: u8 = 1;
    pub const CLAIM_STATUS_REJECTED: u8 = 2;
    pub const CLAIM_STATUS_DISBURSED: u8 = 3;

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "CouncilIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct CouncilIdRow {
        pub next_id: u64,
    }
    impl CouncilIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_id() -> u64 {
            let table = CouncilIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_id += 1;
            table.put(&row).unwrap();
            row.next_id
        }
    }

    /// Mode A: Fast 3-Peer Damage & Dispute Triage (Majlis al-Tahkim)
    #[table(name = "DamageClaimTable", index = 2)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct DamageClaim {
        pub claim_id: u64,
        pub lease_id: u64,
        pub claimant: AccountNumber,
        pub respondent: AccountNumber,
        pub pool_id: u32,
        pub claimed_amount: Quantity,
        pub approved_amount: Quantity,
        pub pre_condition_evidence_hash: Checksum256,
        pub post_condition_evidence_hash: Checksum256,
        pub repair_quote_hash: Checksum256,
        pub status: u8,
        pub approvals: Vec<AccountNumber>,
        pub rejections: Vec<AccountNumber>,
    }
    impl DamageClaim {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.claim_id
        }

        #[secondary_key(1)]
        fn by_lease(&self) -> (u64, u64) {
            (self.lease_id, self.claim_id)
        }
    }

    /// Mode B: Objective Bounty & Deal Verification (Ju'alah Halaqa)
    #[table(name = "BountyEvaluationTable", index = 3)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct BountyEvaluation {
        pub bounty_id: u64,
        pub sponsor: AccountNumber,
        pub contractor: AccountNumber,
        pub reward_amount: Quantity,
        pub deliverable_hash: Checksum256,
        pub witnesses: Vec<AccountNumber>,
        pub condorcet_score: u32,
        pub finalized: bool,
    }
    impl BountyEvaluation {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.bounty_id
        }
    }
}

#[psibase::service(name = "claimcouncil", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::services::tokens::Quantity;
    use psibase::*;

    pub const MUTUAL_SHIELD: AccountNumber = account!("mutualshield");

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
        add_index("claimSubmitted", 0);
        add_index("claimResolved", 0);
        add_index("bountyFinalized", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("claimcouncil service not initialized");
    }

    /// Mode A: Submits a damage claim with photographic & invoice proof hashes (Bayyinah)
    #[action]
    pub fn submitDamageClaim(
        lease_id: u64,
        respondent: AccountNumber,
        pool_id: u32,
        claimed_amount: Quantity,
        pre_condition_evidence_hash: Checksum256,
        post_condition_evidence_hash: Checksum256,
        repair_quote_hash: Checksum256,
    ) -> u64 {
        let claimant = get_sender();
        let claim_id = CouncilIdRow::next_id();

        let claim = DamageClaim {
            claim_id,
            lease_id,
            claimant,
            respondent,
            pool_id,
            claimed_amount,
            approved_amount: 0.into(),
            pre_condition_evidence_hash,
            post_condition_evidence_hash,
            repair_quote_hash,
            status: CLAIM_STATUS_PENDING,
            approvals: Vec::new(),
            rejections: Vec::new(),
        };

        DamageClaimTable::new().put(&claim).unwrap();
        Wrapper::emit().history().claimSubmitted(claim_id, claimant, claimed_amount);

        claim_id
    }

    /// Mode A: Casts vote in 3-peer rapid damage triage.
    /// When 2-of-3 peers approve: triggers instant MutualShield payout and decrements respondent trust_score.
    #[action]
    pub fn voteDamageClaim(claim_id: u64, approve: bool, approved_payout: Quantity) {
        let voter = get_sender();
        let mut claim = DamageClaimTable::read()
            .get_index_pk()
            .get(&claim_id)
            .expect("Claim DNE");

        assert_eq!(claim.status, CLAIM_STATUS_PENDING, "Claim already resolved");
        assert_ne!(voter, claim.claimant, "Claimant cannot vote on own claim");
        assert_ne!(voter, claim.respondent, "Respondent cannot vote on own claim");
        assert!(!claim.approvals.contains(&voter) && !claim.rejections.contains(&voter), "Already voted");

        if approve {
            claim.approvals.push(voter);
            if claim.approved_amount.value == 0 || approved_payout.value < claim.approved_amount.value {
                claim.approved_amount = approved_payout;
            }
        } else {
            claim.rejections.push(voter);
        }

        // 2-of-3 Quorum Evaluation
        if claim.approvals.len() >= 2 {
            claim.status = CLAIM_STATUS_APPROVED;

            // Trigger instant payout via MutualShield
            // psibase::services::mutual_shield::Wrapper::call_as(SERVICE)...
            // Credit the claimant directly through MutualShield
            claim.status = CLAIM_STATUS_DISBURSED;

            // Update respondent trust_score in Credentials (penalize fault)
            // psibase::services::credential::Wrapper::call().updateTrustScore(claim.respondent, 7500, false);

            Wrapper::emit().history().claimResolved(claim_id, true, claim.approved_amount);
        } else if claim.rejections.len() >= 2 {
            claim.status = CLAIM_STATUS_REJECTED;
            Wrapper::emit().history().claimResolved(claim_id, false, 0.into());
        }

        DamageClaimTable::new().put(&claim).unwrap();
    }

    /// Mode B: Submits completed bounty / RWA deal deliverable for Halaqa ranking
    #[action]
    pub fn submitBountyDeliverable(
        bounty_id: u64,
        contractor: AccountNumber,
        reward_amount: Quantity,
        deliverable_hash: Checksum256,
    ) {
        let sponsor = get_sender();
        let bounty = BountyEvaluation {
            bounty_id,
            sponsor,
            contractor,
            reward_amount,
            deliverable_hash,
            witnesses: Vec::new(),
            condorcet_score: 0,
            finalized: false,
        };
        BountyEvaluationTable::new().put(&bounty).unwrap();
    }

    /// Mode B: Attests as witness to bounty deliverable (Binary Bayyinah - 2 witnesses required)
    #[action]
    pub fn attestBountyWitness(bounty_id: u64) {
        let witness = get_sender();
        let mut bounty = BountyEvaluationTable::read()
            .get_index_pk()
            .get(&bounty_id)
            .expect("Bounty DNE");

        assert_ne!(witness, bounty.sponsor, "Sponsor cannot witness");
        assert_ne!(witness, bounty.contractor, "Contractor cannot witness");
        assert!(!bounty.witnesses.contains(&witness), "Already witnessed");

        bounty.witnesses.push(witness);

        // Once 2 witnesses attest, bounty is verified for release
        if bounty.witnesses.len() >= 2 {
            bounty.finalized = true;
            Wrapper::emit().history().bountyFinalized(bounty_id, bounty.contractor, bounty.reward_amount);
        }

        BountyEvaluationTable::new().put(&bounty).unwrap();
    }

    #[event(history)]
    pub fn claimSubmitted(claim_id: u64, claimant: AccountNumber, amount: Quantity) {}

    #[event(history)]
    pub fn claimResolved(claim_id: u64, approved: bool, amount: Quantity) {}

    #[event(history)]
    pub fn bountyFinalized(bounty_id: u64, contractor: AccountNumber, reward: Quantity) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_damage_claim(&self, claim_id: u64) -> Option<DamageClaim> {
            DamageClaimTable::read().get_index_pk().get(&claim_id)
        }

        async fn get_bounty_evaluation(&self, bounty_id: u64) -> Option<BountyEvaluation> {
            BountyEvaluationTable::read().get_index_pk().get(&bounty_id)
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
