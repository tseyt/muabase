#![allow(non_snake_case)]

pub mod tables {
    use async_graphql::SimpleObject;
    use psibase::{AccountNumber, Pack, ToSchema, Unpack};
    use serde::{Deserialize, Serialize};

    #[table(name = "InitTable", index = 0)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct InitRow {}
    impl InitRow {
        #[primary_key]
        fn pk(&self) {}
    }

    #[table(name = "IntentIdTable", index = 1)]
    #[derive(Serialize, Deserialize, ToSchema, Pack, Unpack, Debug, Default)]
    pub struct IntentIdRow {
        pub next_id: u64,
    }
    impl IntentIdRow {
        #[primary_key]
        fn pk(&self) {}

        pub fn next_id() -> u64 {
            let table = IntentIdTable::new();
            let mut row = table.get_index_pk().get(&()).unwrap_or_default();
            row.next_id += 1;
            table.put(&row).unwrap();
            row.next_id
        }
    }

    /// User non-custodial multi-chain & Stripe Connect configuration
    #[table(name = "VaultConfigTable", index = 2)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct VaultConfig {
        pub account: AccountNumber,
        pub btc_taproot_descriptor: String, // Bitcoin P2TR 3-of-5 Miniscript descriptor
        pub evm_safe_address: String,       // EVM Safe address (0x...)
        pub solana_squads_pda: String,      // Solana Squads PDA
        pub stripe_connect_account: String, // Stripe Connect account (acct_...)
        pub charity_sink_stripe: String,    // Public endowment charity sink Stripe account
    }
    impl VaultConfig {
        #[primary_key]
        fn pk(&self) -> AccountNumber {
            self.account
        }
    }

    /// Non-custodial multi-party settlement payload intent
    #[table(name = "SplitIntentTable", index = 3)]
    #[derive(Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone)]
    pub struct SplitIntent {
        pub intent_id: u64,
        pub lease_id: u64,
        pub payer: AccountNumber,
        pub rail_type: String, // "bitcoin_taproot", "evm_safe", "solana_squads", "stripe_connect"
        pub gross_amount_cents: u64,
        pub lender_amount_cents: u64, // 93%
        pub shield_amount_cents: u64, // 4%
        pub saas_amount_cents: u64,   // 3%
        pub payload_json: String,     // Unsigned PSBT hex, Safe multi-send, or Stripe webhook intent
        pub external_tx_ref: Option<String>,
        pub settled: bool,
    }
    impl SplitIntent {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.intent_id
        }

        #[secondary_key(1)]
        fn by_lease(&self) -> (u64, u64) {
            (self.lease_id, self.intent_id)
        }
    }
}

#[psibase::service(name = "ext-vault", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
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
        add_index("configRegistered", 0);
        add_index("intentGenerated", 0);
        add_index("intentSettled", 0);
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        InitTable::read()
            .get_index_pk()
            .get(&())
            .expect("ext-vault service not initialized");
    }

    /// Registers a user's client-side non-custodial wallet & payment credentials
    #[action]
    pub fn registerVaultConfig(
        btc_taproot_descriptor: String,
        evm_safe_address: String,
        solana_squads_pda: String,
        stripe_connect_account: String,
        charity_sink_stripe: String,
    ) {
        let account = get_sender();
        let config = VaultConfig {
            account,
            btc_taproot_descriptor,
            evm_safe_address,
            solana_squads_pda,
            stripe_connect_account,
            charity_sink_stripe,
        };
        VaultConfigTable::new().put(&config).unwrap();
        Wrapper::emit().history().configRegistered(account);
    }

    /// Generates deterministic 93% / 4% / 3% non-custodial multi-party settlement payload
    #[action]
    pub fn generateSplitIntent(
        lease_id: u64,
        rail_type: String,
        gross_amount_cents: u64,
        lender_account: AccountNumber,
    ) -> u64 {
        let payer = get_sender();
        assert!(gross_amount_cents > 0, "Gross amount must be positive");

        // Mathematical Split: 93% Lender, 4% MutualShield, 3% Node Host SaaS
        let lender_amount_cents = (gross_amount_cents * 9300) / 10000;
        let shield_amount_cents = (gross_amount_cents * 400) / 10000;
        let saas_amount_cents = gross_amount_cents - (lender_amount_cents + shield_amount_cents);

        let lender_config = VaultConfigTable::read().get_index_pk().get(&lender_account);
        let stripe_acct = lender_config
            .as_ref()
            .map(|c| c.stripe_connect_account.clone())
            .unwrap_or_else(|| "acct_unregistered".to_string());

        // Construct deterministic settlement payload according to rail
        let payload_json = match rail_type.as_str() {
            "stripe_connect" => {
                serde_json::json!({
                    "payment_method_types": ["card", "apple_pay", "google_pay"],
                    "amount": gross_amount_cents,
                    "currency": "usd",
                    "transfer_data": {
                        "destination": stripe_acct,
                        "amount": lender_amount_cents
                    },
                    "application_fee_amount": saas_amount_cents + shield_amount_cents,
                    "metadata": {
                        "lease_id": lease_id,
                        "shield_allocation_cents": shield_amount_cents,
                        "protocol_saas_cents": saas_amount_cents
                    }
                }).to_string()
            }
            "bitcoin_taproot" => {
                serde_json::json!({
                    "psbt_type": "miniscript_p2tr_3_of_5",
                    "total_sats": gross_amount_cents,
                    "outputs": [
                        { "recipient": "lender_p2tr", "amount_sats": lender_amount_cents },
                        { "recipient": "mutual_shield_p2tr", "amount_sats": shield_amount_cents },
                        { "recipient": "saas_host_p2tr", "amount_sats": saas_amount_cents }
                    ]
                }).to_string()
            }
            "evm_safe" => {
                serde_json::json!({
                    "safe_multisend": true,
                    "token": "USDC_OR_PAXG",
                    "transfers": [
                        { "to": lender_account.to_string(), "amount": lender_amount_cents },
                        { "to": "mutualshield", "amount": shield_amount_cents },
                        { "to": "psinode-saas", "amount": saas_amount_cents }
                    ]
                }).to_string()
            }
            _ => {
                serde_json::json!({
                    "standard": "generic_non_custodial_split",
                    "gross": gross_amount_cents,
                    "splits": {
                        "lender": lender_amount_cents,
                        "shield": shield_amount_cents,
                        "saas": saas_amount_cents
                    }
                }).to_string()
            }
        };

        let intent_id = IntentIdRow::next_id();
        let intent = SplitIntent {
            intent_id,
            lease_id,
            payer,
            rail_type,
            gross_amount_cents,
            lender_amount_cents,
            shield_amount_cents,
            saas_amount_cents,
            payload_json,
            external_tx_ref: None,
            settled: false,
        };

        SplitIntentTable::new().put(&intent).unwrap();
        Wrapper::emit().history().intentGenerated(intent_id, lease_id, payer, gross_amount_cents);

        intent_id
    }

    /// Confirms that external non-custodial transaction has settled on-chain or via Stripe
    #[action]
    pub fn confirmSettlement(intent_id: u64, external_tx_ref: String) {
        let mut intent = SplitIntentTable::read()
            .get_index_pk()
            .get(&intent_id)
            .expect("Intent DNE");

        intent.external_tx_ref = Some(external_tx_ref.clone());
        intent.settled = true;

        SplitIntentTable::new().put(&intent).unwrap();
        Wrapper::emit().history().intentSettled(intent_id, intent.lease_id, external_tx_ref);
    }

    #[event(history)]
    pub fn configRegistered(account: AccountNumber) {}

    #[event(history)]
    pub fn intentGenerated(intent_id: u64, lease_id: u64, payer: AccountNumber, gross: u64) {}

    #[event(history)]
    pub fn intentSettled(intent_id: u64, lease_id: u64, external_ref: String) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_vault_config(&self, account: AccountNumber) -> Option<VaultConfig> {
            VaultConfigTable::read().get_index_pk().get(&account)
        }

        async fn get_split_intent(&self, intent_id: u64) -> Option<SplitIntent> {
            SplitIntentTable::read().get_index_pk().get(&intent_id)
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
