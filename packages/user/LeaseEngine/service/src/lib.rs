#![allow(non_snake_case)]

pub mod domain;
mod tables;
#[cfg(test)]
mod tests;

pub use tables::tables as lease_tables;

#[psibase::service(name = "lease-eng", tables = "tables::tables")]
pub mod service {
    pub use crate::domain::{
        validate_late_return, validate_lease_creation, validate_return_finalization,
        validate_settlement_and_compute_split, DEFAULT_MUTUAL_SHIELD_FEE_BPS,
        DEFAULT_PROTOCOL_SAAS_FEE_BPS,
    };
    use crate::tables::tables::*;
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
        let _ = validate_lease_creation(
            lessee,
            lessor,
            duration_sec,
            gross_payment.value,
            &pre_condition_evidence_hash,
            custodian_account,
            custodian_fee_bps,
            charity_sink_account,
        );

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
            status: STATUS_CREATED,
            is_progressive_buyout,
            buyout_equity_units,
            protocol_saas_fee_bps: DEFAULT_PROTOCOL_SAAS_FEE_BPS,
            mutual_shield_fee_bps: DEFAULT_MUTUAL_SHIELD_FEE_BPS,
            custodian_fee_bps,
            custodian_account,
            charity_sink_account,
        };

        LeaseTable::new().put(&lease).unwrap();
        Wrapper::emit()
            .history()
            .leaseCreated(lease_id, lessor, lessee, gross_payment);

        // Settle payment immediately upon checkout (transitions STATUS_CREATED -> STATUS_ACTIVE)
        settleLeasePayment(lease_id);

        lease_id
    }

    /// Deterministic Multi-Party Revenue Splitter (3% SaaS / 4% MutualShield / Custodian / 93%+ Owner)
    #[action]
    pub fn settleLeasePayment(lease_id: u64) {
        let mut lease = LeaseTable::read()
            .get_index_pk()
            .get(&lease_id)
            .expect("Lease DNE");

        let split = validate_settlement_and_compute_split(&lease, get_sender(), SERVICE);

        let tokens = Tokens::call();

        tokens.debit(
            lease.payment_token_id,
            lease.lessee,
            lease.gross_payment,
            "Lease checkout".into(),
        );

        if split.saas_amount > 0 {
            tokens.credit(
                lease.payment_token_id,
                account!("psi-saas"),
                split.saas_amount.into(),
                "Protocol SaaS fee".into(),
            );
        }

        if split.shield_amount > 0 {
            tokens.credit(
                lease.payment_token_id,
                account!("mut-shield"),
                split.shield_amount.into(),
                "MutualShield contribution".into(),
            );
        }

        if split.custodian_amount > 0 {
            tokens.credit(
                lease.payment_token_id,
                lease.custodian_account,
                split.custodian_amount.into(),
                "Custodian fee".into(),
            );
        }

        tokens.credit(
            lease.payment_token_id,
            lease.lessor,
            split.net_owner_amount.into(),
            "Net owner yield".into(),
        );

        if lease.is_progressive_buyout && lease.buyout_equity_units.value > 0 {
            tokens.debit(
                lease.token_id,
                lease.lessor,
                lease.buyout_equity_units,
                "Diminishing Musharakah Equity Transfer".into(),
            );
            tokens.credit(
                lease.token_id,
                lease.lessee,
                lease.buyout_equity_units,
                "Progressive Equity Acquisition".into(),
            );
        }

        lease.status = STATUS_ACTIVE;
        LeaseTable::new().put(&lease).unwrap();

        Wrapper::emit().history().leaseSettled(
            lease_id,
            split.net_owner_amount.into(),
            split.shield_amount.into(),
            split.saas_amount.into(),
        );
    }

    /// Confirms clean return with post-condition inspection evidence hash
    #[action]
    pub fn confirmCleanReturn(lease_id: u64, post_condition_evidence_hash: Checksum256) {
        let mut lease = LeaseTable::read()
            .get_index_pk()
            .get(&lease_id)
            .expect("Lease DNE");
        validate_return_finalization(&lease, get_sender(), SERVICE, &post_condition_evidence_hash);

        let now = Transact::call().currentBlock().time.seconds();
        lease.returned_time = Some(now);
        lease.post_condition_evidence_hash = Some(post_condition_evidence_hash);
        lease.status = STATUS_RETURNED_CLEAN;

        LeaseTable::new().put(&lease).unwrap();
    }

    /// Hardcoded Anti-Riba Late Return Penalty Engine:
    /// - actual_cost reimburses direct documented loss to lessor
    /// - late_deterrent_penalty routes unconditionally to charity_sink_account (never to lessor)
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
        validate_late_return(&lease, get_sender(), SERVICE, &post_condition_evidence_hash);

        let tokens = Tokens::call();

        if actual_cost.value > 0 {
            tokens.debit(
                lease.payment_token_id,
                lease.lessee,
                actual_cost,
                "Actual direct loss reimbursement".into(),
            );
            tokens.credit(
                lease.payment_token_id,
                lease.lessor,
                actual_cost,
                "Direct loss compensation".into(),
            );
        }

        if late_deterrent_penalty.value > 0 {
            tokens.debit(
                lease.payment_token_id,
                lease.lessee,
                late_deterrent_penalty,
                "Late deterrent surcharge".into(),
            );
            tokens.credit(
                lease.payment_token_id,
                lease.charity_sink_account,
                late_deterrent_penalty,
                "Masraf al-Sadaqah Late Fee Routing".into(),
            );
        }

        let now = Transact::call().currentBlock().time.seconds();
        lease.returned_time = Some(now);
        lease.post_condition_evidence_hash = Some(post_condition_evidence_hash);
        lease.status = STATUS_COMPLETED;

        LeaseTable::new().put(&lease).unwrap();
        Wrapper::emit().history().lateReturnProcessed(
            lease_id,
            actual_cost,
            late_deterrent_penalty,
            lease.charity_sink_account,
        );
    }

    #[event(history)]
    pub fn leaseCreated(
        lease_id: u64,
        lessor: AccountNumber,
        lessee: AccountNumber,
        gross: Quantity,
    ) {
    }

    #[event(history)]
    pub fn leaseSettled(
        lease_id: u64,
        net_owner_yield: Quantity,
        shield_fee: Quantity,
        saas_fee: Quantity,
    ) {
    }

    #[event(history)]
    pub fn lateReturnProcessed(
        lease_id: u64,
        actual_cost: Quantity,
        penalty_to_charity: Quantity,
        charity_sink: AccountNumber,
    ) {
    }

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
