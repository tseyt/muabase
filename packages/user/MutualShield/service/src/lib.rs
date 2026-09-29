#![allow(non_snake_case)]

pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::services::tokens::{Quantity, TID};
    use psibase::{AccountNumber, Checksum256, Pack, TimePointSec, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    /// Mutual Protection Reserve Pool (Takaful Tabarru' Pool)
    #[table(name = "ShieldPoolTable", index = 1)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct ShieldPool {
        pub pool_id: u32,
        pub token_id: TID,
        pub total_contributions: Quantity,
        pub total_claims_paid: Quantity,
        pub reserve_balance: Quantity,
        pub current_epoch: u32,
        pub surplus_rebate_bps: u16, // e.g. 5000 = 50% surplus returned to lenders/renters
    }
    impl ShieldPool {
        #[primary_key]
        fn pk(&self) -> u32 {
            self.pool_id
        }
    }

    /// User Tabarru' contribution record per epoch
    #[table(name = "ContributionRecordTable", index = 2)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct ContributionRecord {
        pub epoch_id: u32,
        pub account: AccountNumber,
        pub contributed_amount: Quantity,
        pub incident_count: u32,
    }
    impl ContributionRecord {
        #[primary_key]
        fn pk(&self) -> (u32, AccountNumber) {
            (self.epoch_id, self.account)
        }
    }

    /// Damage / repair claim disbursement records
    #[table(name = "PayoutRecordTable", index = 3)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct PayoutRecord {
        pub claim_id: u64,
        pub pool_id: u32,
        pub recipient: AccountNumber,
        pub amount: Quantity,
        pub evidence_hash: Checksum256,
        pub authorized_by: AccountNumber,
        pub timestamp: TimePointSec,
    }
    impl PayoutRecord {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.claim_id
        }
    }
}

#[psibase::service(name = "mutualshield", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::services::tokens::{Quantity, TID, Wrapper as Tokens};
    use psibase::services::transact::Wrapper as Transact;
    use psibase::*;

    pub const CLAIM_COUNCIL: AccountNumber = account!("claimcouncil");

    #[action]
    fn init() {
        let table = InitTable::new();
        if table.get_index_pk().get(&()).is_none() {
            table.put(&InitRow {}).unwrap();

            // Default pool 1
            ShieldPoolTable::new().put(&ShieldPool {
                pool_id: 1,
                token_id: 1,
                total_contributions: 0.into(),
                total_claims_paid: 0.into(),
                reserve_balance: 0.into(),
                current_epoch: 1,
                surplus_rebate_bps: 5000,
            }).unwrap();
        }

        let add_index = |method: &str, col: u8| {
            events::Wrapper::call().addIndex(
                EventDb::HistoryEvent,
                Wrapper::SERVICE,
                MethodNumber::from(method),
                col,
            );
        };
        add_index("contributionReceived", 0);
        add_index("payoutDisbursed", 0);
        add_index("surplusDistributed", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("mutualshield service not initialized");
    }

    /// Receives Tabarru' protection contribution from LeaseEngine or guild members
    #[action]
    pub fn contribute(pool_id: u32, amount: Quantity, from: AccountNumber) {
        assert!(amount.value > 0, "Contribution must be positive");
        let mut pool = ShieldPoolTable::read()
            .get_index_pk()
            .get(&pool_id)
            .expect("Pool DNE");

        pool.total_contributions = pool.total_contributions + amount;
        pool.reserve_balance = pool.reserve_balance + amount;
        ShieldPoolTable::new().put(&pool).unwrap();

        let contrib_table = ContributionRecordTable::new();
        let mut record = contrib_table
            .get_index_pk()
            .get(&(pool.current_epoch, from))
            .unwrap_or(ContributionRecord {
                epoch_id: pool.current_epoch,
                account: from,
                contributed_amount: 0.into(),
                incident_count: 0,
            });

        record.contributed_amount = record.contributed_amount + amount;
        contrib_table.put(&record).unwrap();

        Wrapper::emit().history().contributionReceived(pool_id, from, amount);
    }

    /// Disburses instant repair / damage payout, strictly authorized by ClaimCouncil triage
    #[action]
    pub fn disbursePayout(
        claim_id: u64,
        pool_id: u32,
        recipient: AccountNumber,
        amount: Quantity,
        evidence_hash: Checksum256,
    ) {
        let sender = get_sender();
        assert!(
            sender == CLAIM_COUNCIL || sender == SERVICE,
            "Only ClaimCouncil is authorized to disburse Takaful claims"
        );

        let mut pool = ShieldPoolTable::read()
            .get_index_pk()
            .get(&pool_id)
            .expect("Pool DNE");
        assert!(pool.reserve_balance >= amount, "Insufficient shield pool reserves");

        pool.reserve_balance = pool.reserve_balance - amount;
        pool.total_claims_paid = pool.total_claims_paid + amount;
        ShieldPoolTable::new().put(&pool).unwrap();

        let payout = PayoutRecord {
            claim_id,
            pool_id,
            recipient,
            amount,
            evidence_hash,
            authorized_by: sender,
            timestamp: Transact::call().currentBlock().time.seconds(),
        };
        PayoutRecordTable::new().put(&payout).unwrap();

        // Credit recipient from shield reserve
        Tokens::call().credit(pool.token_id, recipient, amount, "MutualShield Takaful Payout".into());

        Wrapper::emit().history().payoutDisbursed(claim_id, recipient, amount);
    }

    /// Distributes underwriting surplus pro-rata back to zero-incident members at epoch end
    #[action]
    pub fn distributeSurplus(pool_id: u32, epoch_id: u32) {
        let mut pool = ShieldPoolTable::read()
            .get_index_pk()
            .get(&pool_id)
            .expect("Pool DNE");
        assert_eq!(epoch_id, pool.current_epoch, "Can only settle current epoch");

        let total_in = pool.total_contributions.value;
        let total_out = pool.total_claims_paid.value;

        if total_in > total_out {
            let surplus = total_in - total_out;
            let distributable = (surplus as u128 * pool.surplus_rebate_bps as u128 / 10000) as i64;
            pool.reserve_balance = pool.reserve_balance - distributable.into();

            Wrapper::emit().history().surplusDistributed(pool_id, epoch_id, distributable.into());
        }

        // Advance epoch
        pool.current_epoch += 1;
        pool.total_contributions = 0.into();
        pool.total_claims_paid = 0.into();
        ShieldPoolTable::new().put(&pool).unwrap();
    }

    #[event(history)]
    pub fn contributionReceived(pool_id: u32, contributor: AccountNumber, amount: Quantity) {}

    #[event(history)]
    pub fn payoutDisbursed(claim_id: u64, recipient: AccountNumber, amount: Quantity) {}

    #[event(history)]
    pub fn surplusDistributed(pool_id: u32, epoch_id: u32, distributable_surplus: Quantity) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_pool(&self, pool_id: u32) -> Option<ShieldPool> {
            ShieldPoolTable::read().get_index_pk().get(&pool_id)
        }

        async fn get_payout(&self, claim_id: u64) -> Option<PayoutRecord> {
            PayoutRecordTable::read().get_index_pk().get(&claim_id)
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
