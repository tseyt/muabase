#![allow(non_snake_case)]

#[psibase::service_tables]
pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::services::tokens::Quantity;
    use psibase::{AccountNumber, Checksum256, Pack, Table, ToSchema, Unpack};
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

#[allow(clippy::too_many_arguments)]
#[psibase::service(name = "claim-coun", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::services::tokens::Quantity;
    use psibase::*;

    /// trust_score deduction (bps) applied to a respondent found at fault by the triage quorum
    const FAULT_TRUST_PENALTY_BPS: u32 = 500;

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
            .expect("claim-coun service not initialized");
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
        assert_ne!(claimant, respondent, "Claimant and respondent must be distinct");
        assert!(claimed_amount.value > 0, "Claimed amount must be positive");
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
        let claim = DamageClaimTable::read()
            .get_index_pk()
            .get(&claim_id)
            .expect("Claim DNE");

        let mut claim = apply_damage_claim_vote(claim, voter, approve, approved_payout);

        if claim.status == CLAIM_STATUS_APPROVED {
            // Trigger instant payout via MutualShield (mut-shield verifies sender == claim-coun)
            psibase::services::mutual_shield::Wrapper::call().disbursePayout(
                claim.claim_id,
                claim.pool_id,
                claim.claimant,
                claim.approved_amount,
                claim.post_condition_evidence_hash.clone(),
            );
            claim.status = CLAIM_STATUS_DISBURSED;

            // Update respondent trust_score in Credentials (penalize fault) relative to
            // their actual current score rather than overwriting it with a fixed value.
            let credentials = psibase::services::credentials::Wrapper::call();
            let current = credentials.trustScoreOf(claim.respondent);
            let penalized = compute_fault_penalized_score(current);
            credentials.updateTrustScore(claim.respondent, penalized, false);

            Wrapper::emit().history().claimResolved(claim_id, true, claim.approved_amount);
        } else if claim.status == CLAIM_STATUS_REJECTED {
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
        assert_ne!(sponsor, contractor, "Sponsor and contractor must be distinct");
        assert!(reward_amount.value > 0, "Reward amount must be positive");
        let table = BountyEvaluationTable::new();
        assert!(
            table.get_index_pk().get(&bounty_id).is_none(),
            "Bounty already exists"
        );
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
        table.put(&bounty).unwrap();
    }

    /// Mode B: Attests as witness to bounty deliverable (Binary Bayyinah - 2 witnesses required)
    #[action]
    pub fn attestBountyWitness(bounty_id: u64) {
        let witness = get_sender();
        let bounty = BountyEvaluationTable::read()
            .get_index_pk()
            .get(&bounty_id)
            .expect("Bounty DNE");

        let bounty = apply_bounty_witness(bounty, witness);
        if bounty.finalized {
            Wrapper::emit().history().bountyFinalized(
                bounty_id,
                bounty.contractor,
                bounty.reward_amount,
            );
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

    pub fn compute_fault_penalized_score(current_score: u32) -> u32 {
        current_score.saturating_sub(FAULT_TRUST_PENALTY_BPS)
    }

    pub fn apply_damage_claim_vote(
        mut claim: DamageClaim,
        voter: AccountNumber,
        approve: bool,
        approved_payout: Quantity,
    ) -> DamageClaim {
        assert_eq!(claim.status, CLAIM_STATUS_PENDING, "Claim already resolved");
        assert_ne!(voter, claim.claimant, "Claimant cannot vote on own claim");
        assert_ne!(voter, claim.respondent, "Respondent cannot vote on own claim");
        assert!(
            !claim.approvals.contains(&voter) && !claim.rejections.contains(&voter),
            "Already voted"
        );

        if approve {
            assert!(
                approved_payout.value > 0 && approved_payout.value <= claim.claimed_amount.value,
                "Approved payout must be within 1..=claimed_amount"
            );
            claim.approvals.push(voter);
            if claim.approved_amount.value == 0
                || approved_payout.value < claim.approved_amount.value
            {
                claim.approved_amount = approved_payout;
            }
        } else {
            claim.rejections.push(voter);
        }

        if claim.approvals.len() >= 2 {
            claim.status = CLAIM_STATUS_APPROVED;
        } else if claim.rejections.len() >= 2 {
            claim.status = CLAIM_STATUS_REJECTED;
        }

        claim
    }

    pub fn apply_bounty_witness(
        mut bounty: BountyEvaluation,
        witness: AccountNumber,
    ) -> BountyEvaluation {
        assert!(!bounty.finalized, "Bounty already finalized");
        assert_ne!(witness, bounty.sponsor, "Sponsor cannot witness");
        assert_ne!(witness, bounty.contractor, "Contractor cannot witness");
        assert!(!bounty.witnesses.contains(&witness), "Already witnessed");

        bounty.witnesses.push(witness);
        if bounty.witnesses.len() >= 2 {
            bounty.finalized = true;
        }
        bounty
    }
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    #[psibase::test_case(packages("ClaimCouncil"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::service::{
        apply_bounty_witness, apply_damage_claim_vote, compute_fault_penalized_score,
    };
    use crate::tables::{
        BountyEvaluation, DamageClaim, CLAIM_STATUS_APPROVED, CLAIM_STATUS_PENDING,
        CLAIM_STATUS_REJECTED,
    };
    use psibase::{account, Checksum256};

    fn sample_claim(claim_id: u64, claimed_amount: u64) -> DamageClaim {
        DamageClaim {
            claim_id,
            lease_id: 100 + claim_id,
            claimant: account!("alice"),
            respondent: account!("bob"),
            pool_id: 1,
            claimed_amount: claimed_amount.into(),
            approved_amount: 0.into(),
            pre_condition_evidence_hash: Checksum256::from([0x11; 32]),
            post_condition_evidence_hash: Checksum256::from([0x22; 32]),
            repair_quote_hash: Checksum256::from([0x33; 32]),
            status: CLAIM_STATUS_PENDING,
            approvals: Vec::new(),
            rejections: Vec::new(),
        }
    }

    #[test]
    fn test_fault_penalty_decrements_relative_to_current_score() {
        assert_eq!(compute_fault_penalized_score(9000), 8500);
        assert_eq!(compute_fault_penalized_score(7500), 7000);
    }

    #[test]
    fn test_fault_penalty_saturates_at_zero() {
        assert_eq!(compute_fault_penalized_score(300), 0);
        assert_eq!(compute_fault_penalized_score(0), 0);
    }

    #[test]
    fn test_two_of_three_approvals_clamp_payout_to_minimum_and_approve_claim() {
        let claim = sample_claim(1, 50_000);

        let after_first = apply_damage_claim_vote(claim, account!("peer1"), true, 45_000.into());
        assert_eq!(after_first.status, CLAIM_STATUS_PENDING);
        assert_eq!(after_first.approved_amount.value, 45_000);

        let after_second =
            apply_damage_claim_vote(after_first, account!("peer2"), true, 40_000.into());
        assert_eq!(after_second.status, CLAIM_STATUS_APPROVED);
        assert_eq!(after_second.approved_amount.value, 40_000);
    }

    #[test]
    fn given_pending_claim_when_two_peers_reject_then_claim_is_rejected_with_zero_payout() {
        let c1 = apply_damage_claim_vote(sample_claim(3, 20_000), account!("peer1"), false, 0.into());
        let c2 = apply_damage_claim_vote(c1, account!("peer2"), false, 0.into());
        assert_eq!(c2.status, CLAIM_STATUS_REJECTED);
        assert_eq!(c2.approved_amount.value, 0);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Approved payout must be within 1..=claimed_amount")]
    fn given_pending_claim_when_approving_payout_exceeds_claimed_or_is_zero_then_rejected() {
        let _ = apply_damage_claim_vote(sample_claim(4, 10_000), account!("peer1"), true, 10_001.into());
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Respondent cannot vote on own claim")]
    fn test_respondent_cannot_vote_on_own_damage_claim() {
        let _ = apply_damage_claim_vote(sample_claim(2, 10_000), account!("bob"), false, 0.into());
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Bounty already finalized")]
    fn given_finalized_bounty_when_extra_witness_attests_then_rejected() {
        let b = BountyEvaluation {
            bounty_id: 1,
            sponsor: account!("sponsor"),
            contractor: account!("dev"),
            reward_amount: 5_000.into(),
            deliverable_hash: Checksum256::from([0xaa; 32]),
            witnesses: Vec::new(),
            condorcet_score: 0,
            finalized: false,
        };
        let b1 = apply_bounty_witness(b, account!("w1"));
        let b2 = apply_bounty_witness(b1, account!("w2"));
        assert!(b2.finalized);
        let _ = apply_bounty_witness(b2, account!("w3"));
    }
}
