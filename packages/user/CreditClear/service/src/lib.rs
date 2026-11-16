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

    /// Non-custodial licensed bridge-country liquidity desk directory entry
    /// (e.g., Turkey, UAE, mainland Ukraine, Central Asia exchange houses publishing Safe/Taproot vaults & FX spreads)
    #[table(name = "BridgeLiquidityDeskTable", index = 4)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct BridgeLiquidityDesk {
        pub desk_id: u64,
        pub operator: AccountNumber,
        /// ISO-3166-1 alpha-2 bridge corridor code (e.g., "TR", "AE", "UA", "UZ")
        pub corridor_jurisdiction: String,
        pub desk_label: String,
        /// 32-byte SHA-256 digest of exchange license / non-custodial attestation (0 PII on-chain)
        pub license_registry_hash: Checksum256,
        pub evm_safe_address: String,
        pub btc_taproot_descriptor: String,
        pub solana_squads_pda: String,
        /// Live bridge FX spread in basis points (100 bps = 1.00%; funded via donor sadaqah_tip for Zakat)
        pub fx_spread_bps: u32,
        pub max_per_tx_usdc_cents: u64,
        pub sadaqah_fee_subsidy_supported: bool,
        pub active: bool,
        pub last_updated: TimePointSec,
    }
    impl BridgeLiquidityDesk {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.desk_id
        }

        #[secondary_key(1)]
        fn by_corridor(&self) -> (String, u64) {
            (self.corridor_jurisdiction.clone(), self.desk_id)
        }

        #[secondary_key(2)]
        fn by_operator(&self) -> (AccountNumber, u64) {
            (self.operator, self.desk_id)
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

    /// Maximum allowed bridge FX spread (10.00% = 1000 bps) to prevent predatory corridor pricing
    pub const MAX_BRIDGE_FX_SPREAD_BPS: u32 = 1000;

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
        add_index("bridgeDeskReg", 0);
        add_index("bridgeDeskUpd", 0);
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

    pub fn validate_bridge_desk_registration(
        corridor_jurisdiction: &str,
        desk_label: &str,
        license_registry_hash: &Checksum256,
        evm_safe_address: &str,
        btc_taproot_descriptor: &str,
        solana_squads_pda: &str,
        fx_spread_bps: u32,
        max_per_tx_usdc_cents: u64,
    ) {
        assert!(
            corridor_jurisdiction.len() == 2
                && corridor_jurisdiction.chars().all(|c| c.is_ascii_uppercase()),
            "corridorJurisdiction '{}' must be a 2-letter uppercase ISO-3166-1 alpha-2 bridge code (e.g. TR, AE, UA, UZ)",
            corridor_jurisdiction
        );
        assert!(
            !desk_label.trim().is_empty() && desk_label.len() <= 96,
            "desk_label must be between 1 and 96 characters"
        );
        assert!(
            *license_registry_hash != Checksum256::from([0u8; 32]),
            "license_registry_hash must be a non-zero 32-byte SHA-256 digest"
        );
        let has_non_custodial_vault = !evm_safe_address.trim().is_empty()
            || !btc_taproot_descriptor.trim().is_empty()
            || !solana_squads_pda.trim().is_empty();
        assert!(
            has_non_custodial_vault,
            "Non-Custodial Safe Harbor Violation: BridgeLiquidityDesk must publish at least one client-verifiable Safe, Taproot, or Squads multisig settlement address"
        );
        assert!(
            fx_spread_bps <= MAX_BRIDGE_FX_SPREAD_BPS,
            "fx_spread_bps {} exceeds maximum corridor spread cap ({} bps)",
            fx_spread_bps,
            MAX_BRIDGE_FX_SPREAD_BPS
        );
        assert!(
            max_per_tx_usdc_cents > 0,
            "max_per_tx_usdc_cents must be strictly positive"
        );
    }

    /// Registers a licensed bridge-country exchange desk publishing non-custodial Safe/Taproot/Squads
    /// settlement addresses and live corridor FX spread metadata without holding platform custody.
    #[action]
    pub fn registerBridgeLiquidityDesk(
        corridor_jurisdiction: String,
        desk_label: String,
        license_registry_hash: Checksum256,
        evm_safe_address: String,
        btc_taproot_descriptor: String,
        solana_squads_pda: String,
        fx_spread_bps: u32,
        max_per_tx_usdc_cents: u64,
        sadaqah_fee_subsidy_supported: bool,
    ) -> u64 {
        let operator = get_sender();
        validate_bridge_desk_registration(
            &corridor_jurisdiction,
            &desk_label,
            &license_registry_hash,
            &evm_safe_address,
            &btc_taproot_descriptor,
            &solana_squads_pda,
            fx_spread_bps,
            max_per_tx_usdc_cents,
        );

        let desk_id = ClearIdRow::next_id();
        let desk = BridgeLiquidityDesk {
            desk_id,
            operator,
            corridor_jurisdiction: corridor_jurisdiction.clone(),
            desk_label,
            license_registry_hash,
            evm_safe_address,
            btc_taproot_descriptor,
            solana_squads_pda,
            fx_spread_bps,
            max_per_tx_usdc_cents,
            sadaqah_fee_subsidy_supported,
            active: true,
            last_updated: Transact::call().currentBlock().time.seconds(),
        };

        BridgeLiquidityDeskTable::new().put(&desk).unwrap();
        Wrapper::emit()
            .history()
            .bridgeDeskReg(desk_id, operator, corridor_jurisdiction, fx_spread_bps);

        desk_id
    }

    /// Updates live FX spread, capacity, or active status for an operator's registered BridgeLiquidityDesk
    #[action]
    pub fn updateBridgeCorridorSpread(
        desk_id: u64,
        fx_spread_bps: u32,
        max_per_tx_usdc_cents: u64,
        active: bool,
    ) {
        let caller = get_sender();
        let mut desk = BridgeLiquidityDeskTable::read()
            .get_index_pk()
            .get(&desk_id)
            .expect("BridgeLiquidityDesk DNE");

        assert_eq!(
            desk.operator, caller,
            "Only the registered desk operator can update corridor spread metadata"
        );
        assert!(
            fx_spread_bps <= MAX_BRIDGE_FX_SPREAD_BPS,
            "fx_spread_bps {} exceeds maximum corridor spread cap ({} bps)",
            fx_spread_bps,
            MAX_BRIDGE_FX_SPREAD_BPS
        );
        assert!(
            max_per_tx_usdc_cents > 0,
            "max_per_tx_usdc_cents must be strictly positive"
        );

        desk.fx_spread_bps = fx_spread_bps;
        desk.max_per_tx_usdc_cents = max_per_tx_usdc_cents;
        desk.active = active;
        desk.last_updated = Transact::call().currentBlock().time.seconds();

        BridgeLiquidityDeskTable::new().put(&desk).unwrap();
        Wrapper::emit()
            .history()
            .bridgeDeskUpd(desk_id, caller, fx_spread_bps, active);
    }

    #[event(history)]
    pub fn invoiceRegistered(invoice_id: u64, creditor: AccountNumber, debtor: AccountNumber, amount: Quantity) {}

    #[event(history)]
    pub fn invoicesNetted(invoice_1: u64, invoice_2: u64, cancelled_amount: Quantity) {}

    #[event(history)]
    pub fn creditLimitUpdated(node_a: AccountNumber, node_b: AccountNumber, limit: Quantity) {}

    #[event(history)]
    pub fn bridgeDeskReg(
        desk_id: u64,
        operator: AccountNumber,
        corridor_jurisdiction: String,
        fx_spread_bps: u32,
    ) {
    }

    #[event(history)]
    pub fn bridgeDeskUpd(desk_id: u64, operator: AccountNumber, fx_spread_bps: u32, active: bool) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_invoice(&self, invoice_id: u64) -> Option<B2BInvoice> {
            B2BInvoiceTable::read().get_index_pk().get(&invoice_id)
        }

        async fn get_credit_limit(&self, node_a: AccountNumber, node_b: AccountNumber) -> Option<WajahahCreditLimit> {
            WajahahCreditTable::read().get_index_pk().get(&(node_a, node_b))
        }

        async fn get_bridge_desk(&self, desk_id: u64) -> Option<BridgeLiquidityDesk> {
            BridgeLiquidityDeskTable::read().get_index_pk().get(&desk_id)
        }

        async fn list_bridge_corridors(
            &self,
            corridor_jurisdiction: Option<String>,
            active_only: Option<bool>,
        ) -> Vec<BridgeLiquidityDesk> {
            let require_active = active_only.unwrap_or(true);
            BridgeLiquidityDeskTable::read()
                .get_index_pk()
                .iter()
                .filter(|desk| {
                    (!require_active || desk.active)
                        && corridor_jurisdiction
                            .as_deref()
                            .map_or(true, |code| desk.corridor_jurisdiction == code)
                })
                .collect()
        }

        async fn bridge_liquidity_desk_table(&self) -> Vec<BridgeLiquidityDesk> {
            BridgeLiquidityDeskTable::read().get_index_pk().iter().collect()
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
    use crate::service::{calculate_wajahah_limit, validate_bridge_desk_registration};
    use psibase::Checksum256;

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

    #[test]
    fn test_bridge_liquidity_desk_accepts_non_custodial_safe_and_taproot() {
        validate_bridge_desk_registration(
            "TR",
            "Bosphorus Non-Custodial Sarf Desk",
            &Checksum256::from([0x5a; 32]),
            "0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1",
            "tr(xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8)",
            "",
            85, // 0.85% FX spread
            2_500_000,
        );
    }

    #[test]
    #[should_panic(
        expected = "Non-Custodial Safe Harbor Violation: BridgeLiquidityDesk must publish at least one client-verifiable Safe, Taproot, or Squads multisig settlement address"
    )]
    fn test_bridge_liquidity_desk_rejects_missing_non_custodial_vaults() {
        validate_bridge_desk_registration(
            "AE",
            "Custodial Desk Without Multisig",
            &Checksum256::from([0x5a; 32]),
            "",
            "   ",
            "",
            50,
            1_000_000,
        );
    }

    #[test]
    #[should_panic(expected = "exceeds maximum corridor spread cap (1000 bps)")]
    fn test_bridge_liquidity_desk_rejects_excessive_fx_spread() {
        validate_bridge_desk_registration(
            "UA",
            "Kyiv Relief Bridge",
            &Checksum256::from([0x5a; 32]),
            "0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1",
            "",
            "",
            1250,
            1_000_000,
        );
    }
}
