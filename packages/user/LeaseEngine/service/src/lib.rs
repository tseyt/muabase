#![allow(non_snake_case)]

pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::services::tokens::{Quantity, TID};
    use psibase::{AccountNumber, Checksum256, Pack, TimePointSec, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    pub const STATUS_CREATED: u8 = 0;
    pub const STATUS_PRE_CONDITION_SIGNED: u8 = 1;
    pub const STATUS_ACTIVE: u8 = 2;
    pub const STATUS_RETURNED_CLEAN: u8 = 3;
    pub const STATUS_DISPUTED: u8 = 4;
    pub const STATUS_COMPLETED: u8 = 5;

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "LeaseIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct LeaseIdRow {
        pub next_id: u64,
    }
    impl LeaseIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_id() -> u64 {
            let table = LeaseIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_id += 1;
            table.put(&row).unwrap();
            row.next_id
        }
    }

    /// Usufruct Lease (Ijarah / P2P Rental / Diminishing Musharakah Buyout)
    #[table(name = "LeaseTable", index = 2)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct UsufructLease {
        pub lease_id: u64,
        pub lessor: AccountNumber,
        pub lessee: AccountNumber,
        pub token_id: TID,
        pub nft_id: Option<u32>,
        pub duration_sec: u64,
        pub gross_payment: Quantity,
        pub payment_token_id: TID,
        pub start_time: Option<TimePointSec>,
        pub due_time: Option<TimePointSec>,
        pub returned_time: Option<TimePointSec>,
        pub pre_condition_evidence_hash: Checksum256,
        pub post_condition_evidence_hash: Option<Checksum256>,
        pub status: u8,
        pub is_progressive_buyout: bool,
        pub buyout_equity_units: Quantity,
        pub protocol_saas_fee_bps: u16,   // e.g. 300 = 3%
        pub mutual_shield_fee_bps: u16,   // e.g. 400 = 4%
        pub custodian_fee_bps: u16,       // e.g. 0 or custom
        pub custodian_account: AccountNumber,
        pub charity_sink_account: AccountNumber,
    }

    impl UsufructLease {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.lease_id
        }

        #[secondary_key(1)]
        fn by_lessee(&self) -> (AccountNumber, u64) {
            (self.lessee, self.lease_id)
        }

        #[secondary_key(2)]
        fn by_lessor(&self) -> (AccountNumber, u64) {
            (self.lessor, self.lease_id)
        }
    }
}

#[psibase::service(name = "lease-eng", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::services::tokens::{Quantity, TID, Wrapper as Tokens};
    use psibase::services::transact::Wrapper as Transact;
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
        add_index("leaseCreated", 0);
        add_index("leaseSettled", 0);
        add_index("lateReturnProcessed", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("lease-eng service not initialized");
    }

    /// Creates a new UsufructLease (P2P rental or progressive buyout)
    #[action]
    pub fn createLease(
        lessor: AccountNumber,
        token_id: TID,
        nft_id: Option<u32>,
        duration_sec: u64,
        gross_payment: Quantity,
        payment_token_id: TID,
        pre_condition_evidence_hash: Checksum256,
        is_progressive_buyout: bool,
        buyout_equity_units: Quantity,
        custodian_account: AccountNumber,
        custodian_fee_bps: u16,
        charity_sink_account: AccountNumber,
    ) -> u64 {
        let lessee = get_sender();
        assert!(gross_payment.value > 0, "Gross payment must be positive");
        assert!(duration_sec > 0, "Duration must be positive");
        assert_ne!(charity_sink_account, lessor, "Charity sink cannot be the lessor");

        let lease_id = LeaseIdRow::next_id();
        let now = Transact::call().currentBlock().time.seconds();
        let due_time = now + psibase::Seconds::new(duration_sec as i64);

        let lease = UsufructLease {
            lease_id,
            lessor,
            lessee,
            token_id,
            nft_id,
            duration_sec,
            gross_payment,
            payment_token_id,
            start_time: Some(now),
            due_time: Some(due_time),
            returned_time: None,
            pre_condition_evidence_hash,
            post_condition_evidence_hash: None,
            status: STATUS_ACTIVE,
            is_progressive_buyout,
            buyout_equity_units,
            protocol_saas_fee_bps: 300, // 3%
            mutual_shield_fee_bps: 400, // 4%
            custodian_fee_bps,
            custodian_account,
            charity_sink_account,
        };

        LeaseTable::new().put(&lease).unwrap();
        Wrapper::emit().history().leaseCreated(lease_id, lessor, lessee, gross_payment);

        // Settle payment immediately upon checkout
        settleLeasePayment(lease_id);

        lease_id
    }

    /// Deterministic Multi-Party Revenue Splitter:
    /// - 3% Protocol SaaS
    /// - 4% MutualShield Takaful damage pool
    /// - Custodian Fee (if any)
    /// - 93%+ Net Owner Yield (pro-rata to co-owners)
    /// - If progressive buyout, transfer equity units
    #[action]
    pub fn settleLeasePayment(lease_id: u64) {
        let lease = LeaseTable::read()
            .get_index_pk()
            .get(&lease_id)
            .expect("Lease DNE");

        let gross = lease.gross_payment.value as u128;
        let protocol_fee = (gross * lease.protocol_saas_fee_bps as u128 / 10000) as i64;
        let shield_fee = (gross * lease.mutual_shield_fee_bps as u128 / 10000) as i64;
        let custodian_fee = (gross * lease.custodian_fee_bps as u128 / 10000) as i64;

        let total_deductions = protocol_fee + shield_fee + custodian_fee;
        let net_owner_yield = (gross as i64) - total_deductions;
        assert!(net_owner_yield > 0, "Net yield calculation invalid");

        let tokens = Tokens::call();

        // Debit lessee
        tokens.debit(lease.payment_token_id, lease.lessee, lease.gross_payment, "Lease checkout".into());

        // Route SaaS fee (3%)
        if protocol_fee > 0 {
            tokens.credit(lease.payment_token_id, account!("psinode-saas"), protocol_fee.into(), "Protocol SaaS fee".into());
        }

        // Route MutualShield pool contribution (4% Tabarru')
        if shield_fee > 0 {
            tokens.credit(lease.payment_token_id, account!("mutualshield"), shield_fee.into(), "MutualShield contribution".into());
        }

        // Route Custodian maintenance fee if configured
        if custodian_fee > 0 && lease.custodian_account.value != 0 {
            tokens.credit(lease.payment_token_id, lease.custodian_account, custodian_fee.into(), "Custodian fee".into());
        }

        // Route net yield to lessor / co-owners (93%+)
        tokens.credit(lease.payment_token_id, lease.lessor, net_owner_yield.into(), "Net owner yield".into());

        // If progressive buyout (Diminishing Musharakah), simultaneously transfer equity units to lessee
        if lease.is_progressive_buyout && lease.buyout_equity_units.value > 0 {
            tokens.debit(lease.token_id, lease.lessor, lease.buyout_equity_units, "Diminishing Musharakah Equity Transfer".into());
            tokens.credit(lease.token_id, lease.lessee, lease.buyout_equity_units, "Progressive Equity Acquisition".into());
        }

        Wrapper::emit().history().leaseSettled(lease_id, net_owner_yield.into(), shield_fee.into(), protocol_fee.into());
    }

    /// Confirms clean return with post-condition inspection evidence hash
    #[action]
    pub fn confirmCleanReturn(lease_id: u64, post_condition_evidence_hash: Checksum256) {
        let mut lease = LeaseTable::read()
            .get_index_pk()
            .get(&lease_id)
            .expect("Lease DNE");
        assert_eq!(lease.status, STATUS_ACTIVE, "Lease not active");

        let now = Transact::call().currentBlock().time.seconds();
        lease.returned_time = Some(now);
        lease.post_condition_evidence_hash = Some(post_condition_evidence_hash);
        lease.status = STATUS_RETURNED_CLEAN;

        LeaseTable::new().put(&lease).unwrap();
    }

    /// Hardcoded Anti-Riba Late Return Penalty Engine:
    /// - actual_cost is credited to lessor / repairer (reimbursing direct documented damages)
    /// - late_deterrent_penalty CAN NEVER ACCRUE TO LESSOR! It is UNCONDITIONALLY CREDITED TO charity_sink_account.
    #[action]
    pub fn processLateReturn(
        lease_id: u64,
        actual_cost: Quantity,
        late_deterrent_penalty: Quantity,
        post_condition_evidence_hash: Checksum256,
    ) {
        let mut lease = LeaseTable::read()
            .get_index_pk()
            .get(&lease_id)
            .expect("Lease DNE");

        let tokens = Tokens::call();

        // 1. Direct actual reimbursement (rebooking, cleaning, repair) goes to lessor
        if actual_cost.value > 0 {
            tokens.debit(lease.payment_token_id, lease.lessee, actual_cost, "Actual direct loss reimbursement".into());
            tokens.credit(lease.payment_token_id, lease.lessor, actual_cost, "Direct loss compensation".into());
        }

        // 2. ANTI-RIBA ZERO-CREDITOR INVARIANT:
        // Any punitive deterrent penalty MUST be routed to charity_sink_account.
        // Assert that the lessor is NOT the charity sink account.
        assert_ne!(
            lease.charity_sink_account, lease.lessor,
            "Riba Violation: Creditor/lessor can never receive late penalties"
        );

        if late_deterrent_penalty.value > 0 {
            tokens.debit(lease.payment_token_id, lease.lessee, late_deterrent_penalty, "Late deterrent surcharge".into());
            tokens.credit(lease.payment_token_id, lease.charity_sink_account, late_deterrent_penalty, "Masraf al-Sadaqah Late Fee Routing".into());
        }

        let now = Transact::call().currentBlock().time.seconds();
        lease.returned_time = Some(now);
        lease.post_condition_evidence_hash = Some(post_condition_evidence_hash);
        lease.status = STATUS_COMPLETED;

        LeaseTable::new().put(&lease).unwrap();
        Wrapper::emit().history().lateReturnProcessed(lease_id, actual_cost, late_deterrent_penalty, lease.charity_sink_account);
    }

    #[event(history)]
    pub fn leaseCreated(lease_id: u64, lessor: AccountNumber, lessee: AccountNumber, gross: Quantity) {}

    #[event(history)]
    pub fn leaseSettled(lease_id: u64, net_owner_yield: Quantity, shield_fee: Quantity, saas_fee: Quantity) {}

    #[event(history)]
    pub fn lateReturnProcessed(lease_id: u64, actual_cost: Quantity, penalty_to_charity: Quantity, charity_sink: AccountNumber) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_lease(&self, lease_id: u64) -> Option<UsufructLease> {
            LeaseTable::read().get_index_pk().get(&lease_id)
        }

        async fn leases_by_lessee(&self, lessee: AccountNumber) -> Vec<UsufructLease> {
            LeaseTable::read()
                .get_index_by_lessee()
                .range((lessee, 0)..=(lessee, u64::MAX))
                .collect()
        }

        async fn leases_by_lessor(&self, lessor: AccountNumber) -> Vec<UsufructLease> {
            LeaseTable::read()
                .get_index_by_lessor()
                .range((lessor, 0)..=(lessor, u64::MAX))
                .collect()
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
