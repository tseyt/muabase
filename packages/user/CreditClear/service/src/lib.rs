#![allow(non_snake_case)]

#[psibase::service_tables]
pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::services::tokens::{Quantity, TID};
    use psibase::{AccountNumber, Checksum256, Pack, Table, TimePointSec, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "ClearIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct ClearIdRow {
        pub next_id: u64,
    }
    impl ClearIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_id() -> u64 {
            let table = ClearIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_id += 1;
            table.put(&row).unwrap();
            row.next_id
        }
    }

    /// B2B Trade Invoices for multilateral netting (Muqassah)
    #[table(name = "B2BInvoiceTable", index = 2)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct B2BInvoice {
        pub invoice_id: u64,
        pub creditor: AccountNumber,
        pub debtor: AccountNumber,
        pub token_id: TID,
        pub gross_amount: Quantity,
        pub remaining_balance: Quantity,
        pub due_date: TimePointSec,
        pub invoice_hash: Checksum256,
        pub settled: bool,
    }
    impl B2BInvoice {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.invoice_id
        }

        #[secondary_key(1)]
        fn by_debtor(&self) -> (AccountNumber, u64) {
            (self.debtor, self.invoice_id)
        }

        #[secondary_key(2)]
        fn by_creditor(&self) -> (AccountNumber, u64) {
            (self.creditor, self.invoice_id)
        }
    }

    /// Wajahah-Hawalah synthetic credit line parameters
    #[table(name = "WajahahCreditTable", index = 3)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct WajahahCreditLimit {
        pub node_a: AccountNumber,
        pub node_b: AccountNumber,
        pub settled_volume_90d: Quantity,
        pub disputed_volume: Quantity,
        pub waqf_reserve: Quantity,
        pub credit_limit: Quantity,
        pub current_outstanding: Quantity,
        pub last_calculated: TimePointSec,
    }
    impl WajahahCreditLimit {
        #[primary_key]
        fn pk(&self) -> (AccountNumber, AccountNumber) {
            (self.node_a, self.node_b)
        }
    }
}

#[psibase::service(name = "cred-clear", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::services::tokens::{Quantity, TID};
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
        add_index("invoiceRegistered", 0);
        add_index("invoicesNetted", 0);
        add_index("creditLimitUpdated", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("creditclear service not initialized");
    }

    /// Registers a B2B commercial invoice for netting
    #[action]
    pub fn registerInvoice(
        debtor: AccountNumber,
        token_id: TID,
        gross_amount: Quantity,
        due_date: TimePointSec,
        invoice_hash: Checksum256,
    ) -> u64 {
        let creditor = get_sender();
        assert_ne!(creditor, debtor, "Creditor and debtor cannot be identical");
        assert!(gross_amount.value > 0, "Invoice amount must be positive");

        let invoice_id = ClearIdRow::next_id();
        let invoice = B2BInvoice {
            invoice_id,
            creditor,
            debtor,
            token_id,
            gross_amount,
            remaining_balance: gross_amount,
            due_date,
            invoice_hash,
            settled: false,
        };

        B2BInvoiceTable::new().put(&invoice).unwrap();
        Wrapper::emit().history().invoiceRegistered(invoice_id, creditor, debtor, gross_amount);

        invoice_id
    }

    /// Computes inter-chain Wajahah-Hawalah synthetic credit line:
    /// L(A -> B) = min( alpha * V_settled(A)_90d * (1 - D_A)^k,  beta * R_Waqf(A) )
    #[action]
    pub fn computeWajahahLimit(
        node_a: AccountNumber,
        node_b: AccountNumber,
        settled_volume_90d: Quantity,
        disputed_volume: Quantity,
        waqf_reserve: Quantity,
    ) -> Quantity {
        let credit_limit: u64 = calculate_wajahah_limit(
            settled_volume_90d.value,
            disputed_volume.value,
            waqf_reserve.value,
        );

        let table = WajahahCreditTable::new();
        let record = WajahahCreditLimit {
            node_a,
            node_b,
            settled_volume_90d,
            disputed_volume,
            waqf_reserve,
            credit_limit: credit_limit.into(),
            current_outstanding: 0.into(),
            last_calculated: Transact::call().currentBlock().time.seconds(),
        };
        table.put(&record).unwrap();

        Wrapper::emit().history().creditLimitUpdated(node_a, node_b, credit_limit.into());
        credit_limit.into()
    }

    pub fn calculate_wajahah_limit(
        settled_volume_90d: u64,
        disputed_volume: u64,
        waqf_reserve: u64,
    ) -> u64 {
        let alpha_bps: u64 = 2500; // alpha = 0.25 (25%)
        let beta_bps: u64 = 10000; // beta = 1.0 (100%)

        let v_settled = settled_volume_90d as u128;
        let v_disputed = disputed_volume as u128;

        // Compute dispute discount factor: (1 - D_A)^2
        // D_A = min(v_disputed / max(1, v_settled), 1)
        let dispute_ratio_ppm = if v_settled > 0 {
            std::cmp::min(1_000_000, (v_disputed * 1_000_000) / v_settled)
        } else {
            1_000_000
        };

        let trust_factor_ppm = 1_000_000 - dispute_ratio_ppm;
        // (1 - D_A)^2
        let trust_decay_ppm = (trust_factor_ppm * trust_factor_ppm) / 1_000_000;

        let volume_capacity = (v_settled * alpha_bps as u128 / 10000) * trust_decay_ppm / 1_000_000;
        let reserve_capacity = (waqf_reserve as u128 * beta_bps as u128) / 10000;

        std::cmp::min(volume_capacity, reserve_capacity) as u64
    }

    /// Executes bilateral/multilateral set-off (Muqassah), cancelling reciprocal debt obligations
    #[action]
    pub fn executeBilateralNetting(invoice_1_id: u64, invoice_2_id: u64) {
        let mut inv1 = B2BInvoiceTable::read()
            .get_index_pk()
            .get(&invoice_1_id)
            .expect("Invoice 1 DNE");
        let mut inv2 = B2BInvoiceTable::read()
            .get_index_pk()
            .get(&invoice_2_id)
            .expect("Invoice 2 DNE");

        assert_eq!(inv1.token_id, inv2.token_id, "Invoices must use identical denomination token");
        assert_eq!(inv1.creditor, inv2.debtor, "Invoices must be reciprocal counter-parties");
        assert_eq!(inv1.debtor, inv2.creditor, "Invoices must be reciprocal counter-parties");
        assert!(!inv1.settled && !inv2.settled, "Invoices already settled");

        // Netting amount is the min of the two balances (extinguishing mutual obligations)
        let net_cancel = std::cmp::min(inv1.remaining_balance.value, inv2.remaining_balance.value);

        inv1.remaining_balance = inv1.remaining_balance - net_cancel.into();
        inv2.remaining_balance = inv2.remaining_balance - net_cancel.into();

        if inv1.remaining_balance.value == 0 {
            inv1.settled = true;
        }
        if inv2.remaining_balance.value == 0 {
            inv2.settled = true;
        }

        B2BInvoiceTable::new().put(&inv1).unwrap();
        B2BInvoiceTable::new().put(&inv2).unwrap();

        Wrapper::emit().history().invoicesNetted(invoice_1_id, invoice_2_id, net_cancel.into());
    }

    #[event(history)]
    pub fn invoiceRegistered(invoice_id: u64, creditor: AccountNumber, debtor: AccountNumber, amount: Quantity) {}

    #[event(history)]
    pub fn invoicesNetted(invoice_1: u64, invoice_2: u64, cancelled_amount: Quantity) {}

    #[event(history)]
    pub fn creditLimitUpdated(node_a: AccountNumber, node_b: AccountNumber, limit: Quantity) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_invoice(&self, invoice_id: u64) -> Option<B2BInvoice> {
            B2BInvoiceTable::read().get_index_pk().get(&invoice_id)
        }

        async fn get_credit_limit(&self, node_a: AccountNumber, node_b: AccountNumber) -> Option<WajahahCreditLimit> {
            WajahahCreditTable::read().get_index_pk().get(&(node_a, node_b))
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
    use crate::service::calculate_wajahah_limit;

    #[test]
    fn test_wajahah_limit_zero_disputes_capped_by_volume_or_reserve() {
        // 100,000 settled, 0 disputed -> 25% = 25,000 volume capacity; reserve = 50,000 -> limit = 25,000
        assert_eq!(calculate_wajahah_limit(100_000, 0, 50_000), 25_000);
        // Capped by waqf reserve when reserve < volume capacity
        assert_eq!(calculate_wajahah_limit(100_000, 0, 10_000), 10_000);
    }

    #[test]
    fn test_wajahah_limit_quadratic_dispute_decay() {
        // 100,000 settled, 20,000 disputed -> D_A = 0.2 -> (1 - 0.2)^2 = 0.64
        // Volume capacity = 25,000 * 0.64 = 16,000
        assert_eq!(calculate_wajahah_limit(100_000, 20_000, 50_000), 16_000);
        // 100% disputed -> limit collapses to 0
        assert_eq!(calculate_wajahah_limit(100_000, 100_000, 50_000), 0);
    }
}
