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

    /// Corridor split routing mode:
    /// - `COMMERCIAL_93_4_3`: Standard commercial lease split (93% owner / 4% mutual_shield / 3% SaaS).
    /// - `ZAKAT_TAMLIK_100`: Shariah-compliant Zakat *Tamlik* routing (100% principal to `recipientBridgeVault`,
    ///   0% protocol fee deduction from `zakatPrincipalAmount`, plus optional separate `sadaqahTipAmount`).
    #[allow(non_camel_case_types)]
    #[derive(
        async_graphql::Enum, Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Debug, Default,
    )]
    pub enum CorridorSplitMode {
        #[default]
        #[graphql(name = "COMMERCIAL_93_4_3")]
        COMMERCIAL_93_4_3,
        #[graphql(name = "ZAKAT_TAMLIK_100")]
        ZAKAT_TAMLIK_100,
    }

    impl CorridorSplitMode {
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::COMMERCIAL_93_4_3 => "COMMERCIAL_93_4_3",
                Self::ZAKAT_TAMLIK_100 => "ZAKAT_TAMLIK_100",
            }
        }

        pub fn parse_str(value: &str) -> Result<Self, psibase::fracpack::Error> {
            match value {
                "COMMERCIAL_93_4_3" => Ok(Self::COMMERCIAL_93_4_3),
                "ZAKAT_TAMLIK_100" => Ok(Self::ZAKAT_TAMLIK_100),
                _ => Err(psibase::fracpack::Error::BadScalar),
            }
        }
    }

    impl psibase::fracpack::Pack for CorridorSplitMode {
        const FIXED_SIZE: u32 = 4;
        const VARIABLE_SIZE: bool = true;

        fn pack(&self, dest: &mut Vec<u8>) {
            self.as_str().pack(dest);
        }
    }

    impl<'a> psibase::fracpack::Unpack<'a> for CorridorSplitMode {
        const FIXED_SIZE: u32 = 4;
        const VARIABLE_SIZE: bool = true;

        fn unpack(
            src: &mut psibase::fracpack::FracInputStream<'a>,
        ) -> psibase::fracpack::Result<Self> {
            let raw = <&str as psibase::fracpack::Unpack>::unpack(src)?;
            Self::parse_str(raw)
        }

        fn verify(src: &mut psibase::fracpack::FracInputStream) -> psibase::fracpack::Result<()> {
            let raw = <&str as psibase::fracpack::Unpack>::unpack(src)?;
            Self::parse_str(raw).map(|_| ())
        }
    }

    impl psibase::fracpack::ToSchema for CorridorSplitMode {
        fn schema(builder: &mut psibase::fracpack::SchemaBuilder) -> psibase::fracpack::AnyType {
            <String as psibase::fracpack::ToSchema>::schema(builder)
        }
    }

    /// Non-custodial Zakat *Tamlik* & humanitarian bridge remittance intent (0 platform keys held)
    #[table(name = "RemittanceIntentTable", index = 4)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct ZakatRemittanceIntent {
        pub intent_id: u64,
        pub attestation_id: u64,
        pub donor: AccountNumber,
        pub mode: CorridorSplitMode,
        pub rail_type: String, // "evm_safe", "bitcoin_taproot", "solana_squads", "bridge_corridor_api"
        pub corridor_jurisdiction: String, // e.g., "TR", "AE", "UA", "UZ"
        pub recipient_bridge_vault: String,
        pub infra_ops_vault: Option<String>,
        pub zakat_principal_amount: u64,
        pub recipient_amount: u64,
        pub sadaqah_tip_amount: u64,
        pub protocol_fee_deducted: u64,
        pub recipient_commitment_hash: Checksum256,
        pub payload_json: String,
        pub external_tx_ref: Option<String>,
        pub settled: bool,
    }
    impl ZakatRemittanceIntent {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.intent_id
        }

        #[secondary_key(1)]
        fn by_attestation(&self) -> (u64, u64) {
            (self.attestation_id, self.intent_id)
        }
    }

    /// Tamper-evident double-entry compliance receipt stored on settlement (zero plaintext PII)
    #[table(name = "CorridorReceiptTable", index = 5)]
    #[derive(
        Serialize, Deserialize, SimpleObject, ToSchema, Pack, Unpack, Debug, Clone, PartialEq, Eq,
    )]
    pub struct CorridorReceipt {
        pub intent_id: u64,
        pub attestation_id: u64,
        pub corridor_jurisdiction: String,
        pub recipient_amount: u64,
        pub sadaqah_tip_amount: u64,
        pub protocol_fee_deducted: u64,
        pub recipient_commitment_hash: Checksum256,
        pub external_tx_ref: String,
        pub receipt_leaf_hash: Checksum256,
    }
    impl CorridorReceipt {
        #[primary_key]
        fn pk(&self) -> u64 {
            self.intent_id
        }
    }

    #[derive(Serialize, Deserialize, SimpleObject, Debug, Clone, PartialEq, Eq, Default)]
    pub struct CorridorConservation {
        pub corridor_jurisdiction: String,
        pub settled_count: u64,
        pub total_principal_delivered: u64,
        pub total_sadaqah_tips: u64,
        pub total_fees_deducted: u64,
    }
}

#[psibase::service(name = "ext-vault", tables = "tables")]
pub mod service {
    use crate::tables::*;
    use async_graphql::Object;
    use psibase::services::events;
    use psibase::services::staged_tx::Wrapper as StagedTxSvc;
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
        add_index("remitGenerated", 0);
        add_index("remitSettled", 0);
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
        let (lender_amount_cents, shield_amount_cents, saas_amount_cents) =
            compute_split_amounts(gross_amount_cents);

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

    /// Generates a non-custodial Zakat *Tamlik* or Humanitarian Corridor settlement split intent.
    ///
    /// Invariants enforced before state commit:
    /// 1. **Tamlik Principal Invariant:** If `mode == CorridorSplitMode::ZAKAT_TAMLIK_100`, panics if
    ///    `protocol_fee_deduction > 0`. 100% of `zakat_principal_amount` routes directly to
    ///    `recipient_bridge_vault`; optional `sadaqah_tip_amount` routes separately to `infra_ops_vault` / `charity_sink`.
    /// 2. **Zero-PII Commitment Guard:** Rejects zeroed 32-byte `recipient_commitment_hash` or non-ISO corridor strings.
    /// 3. **Dual-Witness Evidence Gate:** Verifies `StagedTx::verifyRemittanceQuorum(attestation_id, recipient_commitment_hash)`
    ///    (`witness_quorum >= 2`, active `"humanitarian-verifier"` soulbound badge, `trust_score >= 8000`).
    #[action]
    pub fn createZakatRemittanceSplit(
        attestation_id: u64,
        mode: CorridorSplitMode,
        rail_type: String,
        corridor_jurisdiction: String,
        recipient_bridge_vault: String,
        infra_ops_vault: Option<String>,
        zakat_principal_amount: u64,
        sadaqah_tip_amount: u64,
        protocol_fee_deduction: u64,
        recipient_commitment_hash: Checksum256,
    ) -> u64 {
        let donor = get_sender();

        let (recipient_amount, tip_routed_amount, fee_deducted) = validate_zakat_remittance_split(
            mode,
            &corridor_jurisdiction,
            &recipient_bridge_vault,
            zakat_principal_amount,
            sadaqah_tip_amount,
            protocol_fee_deduction,
            &recipient_commitment_hash,
        );

        // Enforce witness_quorum >= 2 with active "humanitarian-verifier" badges & trust_score >= 8000
        let quorum_ok = StagedTxSvc::call()
            .verifyRemittanceQuorum(attestation_id, recipient_commitment_hash.clone());
        assert!(
            quorum_ok,
            "Remittance settlement blocked: beneficiary attestation quorum not satisfied"
        );

        let resolved_infra_vault = if tip_routed_amount > 0 {
            Some(
                infra_ops_vault
                    .clone()
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| "charity-sink".to_string()),
            )
        } else {
            infra_ops_vault.filter(|v| !v.trim().is_empty())
        };

        let payload_json = build_remittance_multisig_payload(
            mode,
            &rail_type,
            &corridor_jurisdiction,
            &recipient_bridge_vault,
            resolved_infra_vault.as_deref(),
            recipient_amount,
            tip_routed_amount,
            fee_deducted,
            attestation_id,
            &recipient_commitment_hash,
        );

        let intent_id = IntentIdRow::next_id();
        let intent = ZakatRemittanceIntent {
            intent_id,
            attestation_id,
            donor,
            mode,
            rail_type,
            corridor_jurisdiction,
            recipient_bridge_vault,
            infra_ops_vault: resolved_infra_vault,
            zakat_principal_amount,
            recipient_amount,
            sadaqah_tip_amount: tip_routed_amount,
            protocol_fee_deducted: fee_deducted,
            recipient_commitment_hash,
            payload_json,
            external_tx_ref: None,
            settled: false,
        };

        RemittanceIntentTable::new().put(&intent).unwrap();
        Wrapper::emit().history().remitGenerated(
            intent_id,
            attestation_id,
            donor,
            recipient_amount,
            tip_routed_amount,
        );

        intent_id
    }

    /// Confirms external non-custodial execution of a Zakat / Humanitarian remittance split
    /// (e.g., EVM Safe tx hash, Bitcoin Taproot txid, Solana Squads signature, or licensed bridge receipt hash).
    #[action]
    pub fn confirmRemittanceSettlement(intent_id: u64, external_tx_ref: String) {
        assert!(
            !external_tx_ref.trim().is_empty(),
            "external_tx_ref settlement confirmation hash must be non-empty"
        );

        let mut intent = RemittanceIntentTable::read()
            .get_index_pk()
            .get(&intent_id)
            .expect("ZakatRemittanceIntent DNE");

        validate_settlement_caller(get_sender(), intent.donor, SERVICE, intent.settled);

        // Re-assert Tamlik 100% principal delivery & dual-witness quorum prior to marking settled
        if intent.mode == CorridorSplitMode::ZAKAT_TAMLIK_100 {
            assert!(
                intent.protocol_fee_deducted == 0
                    && intent.recipient_amount == intent.zakat_principal_amount,
                "Tamlik Violation: ZAKAT_TAMLIK_100 intent cannot settle with fee deduction"
            );
        }

        let quorum_ok = StagedTxSvc::call().verifyRemittanceQuorum(
            intent.attestation_id,
            intent.recipient_commitment_hash.clone(),
        );
        assert!(
            quorum_ok,
            "Remittance settlement confirmation blocked: witness quorum not satisfied"
        );

        let receipt = build_corridor_receipt(&intent, &external_tx_ref);

        intent.external_tx_ref = Some(external_tx_ref.clone());
        intent.settled = true;

        RemittanceIntentTable::new().put(&intent).unwrap();
        CorridorReceiptTable::new().put(&receipt).unwrap();
        Wrapper::emit().history().remitSettled(
            intent_id,
            intent.attestation_id,
            external_tx_ref,
        );
        Wrapper::emit()
            .history()
            .complianceReceipt(intent_id, receipt.receipt_leaf_hash);
    }

    pub fn validate_settlement_caller(
        sender: AccountNumber,
        donor: AccountNumber,
        service_acct: AccountNumber,
        already_settled: bool,
    ) {
        assert!(
            !already_settled,
            "ZakatRemittanceIntent has already been settled"
        );
        assert!(
            sender == donor || sender == service_acct,
            "Unauthorized settlement confirmation: caller '{}' must be donor '{}' or '{}'",
            sender,
            donor,
            service_acct
        );
    }

    pub fn build_corridor_receipt(
        intent: &ZakatRemittanceIntent,
        external_tx_ref: &str,
    ) -> CorridorReceipt {
        use muabase_institutional::{JournalEntry, JournalLine};

        let total_out = intent
            .recipient_amount
            .saturating_add(intent.sadaqah_tip_amount)
            .saturating_add(intent.protocol_fee_deducted);
        let mut lines = vec![
            JournalLine {
                account: format!("donor:{}", intent.donor),
                delta: -(total_out as i64),
            },
            JournalLine {
                account: format!("bridge:{}", intent.recipient_bridge_vault),
                delta: intent.recipient_amount as i64,
            },
        ];
        if intent.sadaqah_tip_amount > 0 {
            lines.push(JournalLine {
                account: format!(
                    "ops:{}",
                    intent.infra_ops_vault.as_deref().unwrap_or("charity-sink")
                ),
                delta: intent.sadaqah_tip_amount as i64,
            });
        }
        if intent.protocol_fee_deducted > 0 {
            lines.push(JournalLine {
                account: "protocol:fee".to_string(),
                delta: intent.protocol_fee_deducted as i64,
            });
        }

        let entry = JournalEntry {
            entry_id: intent.intent_id,
            category: format!("corridor:{}", intent.corridor_jurisdiction),
            lines,
            commitment_hash: intent.recipient_commitment_hash.0,
        };
        let leaf = entry
            .receipt_leaf_hash("receipt/corridor")
            .expect("Double-entry corridor conservation violated");

        CorridorReceipt {
            intent_id: intent.intent_id,
            attestation_id: intent.attestation_id,
            corridor_jurisdiction: intent.corridor_jurisdiction.clone(),
            recipient_amount: intent.recipient_amount,
            sadaqah_tip_amount: intent.sadaqah_tip_amount,
            protocol_fee_deducted: intent.protocol_fee_deducted,
            recipient_commitment_hash: intent.recipient_commitment_hash.clone(),
            external_tx_ref: external_tx_ref.to_string(),
            receipt_leaf_hash: Checksum256::from(leaf),
        }
    }

    #[event(history)]
    pub fn configRegistered(account: AccountNumber) {}

    #[event(history)]
    pub fn intentGenerated(intent_id: u64, lease_id: u64, payer: AccountNumber, gross: u64) {}

    #[event(history)]
    pub fn intentSettled(intent_id: u64, lease_id: u64, external_ref: String) {}

    #[event(history)]
    pub fn remitGenerated(
        intent_id: u64,
        attestation_id: u64,
        donor: AccountNumber,
        principal: u64,
        sadaqah_tip: u64,
    ) {
    }

    #[event(history)]
    pub fn remitSettled(intent_id: u64, attestation_id: u64, external_ref: String) {}

    #[event(history)]
    pub fn complianceReceipt(intent_id: u64, receipt_leaf_hash: Checksum256) {}

    struct Query;

    #[Object]
    impl Query {
        async fn get_vault_config(&self, account: AccountNumber) -> Option<VaultConfig> {
            VaultConfigTable::read().get_index_pk().get(&account)
        }

        async fn get_split_intent(&self, intent_id: u64) -> Option<SplitIntent> {
            SplitIntentTable::read().get_index_pk().get(&intent_id)
        }

        async fn get_remittance_intent(&self, intent_id: u64) -> Option<ZakatRemittanceIntent> {
            RemittanceIntentTable::read().get_index_pk().get(&intent_id)
        }

        async fn remittance_intent_table(
            &self,
            corridor_jurisdiction: Option<String>,
        ) -> Vec<ZakatRemittanceIntent> {
            RemittanceIntentTable::read()
                .get_index_pk()
                .iter()
                .filter(|row| {
                    corridor_jurisdiction
                        .as_deref()
                        .map_or(true, |code| row.corridor_jurisdiction == code)
                })
                .collect()
        }

        async fn corridor_receipt(&self, intent_id: u64) -> Option<CorridorReceipt> {
            CorridorReceiptTable::read().get_index_pk().get(&intent_id)
        }

        async fn corridor_conservation(
            &self,
            corridor_jurisdiction: String,
        ) -> CorridorConservation {
            let mut agg = CorridorConservation {
                corridor_jurisdiction: corridor_jurisdiction.clone(),
                ..Default::default()
            };
            for r in CorridorReceiptTable::read().get_index_pk().iter() {
                if r.corridor_jurisdiction == corridor_jurisdiction {
                    agg.settled_count += 1;
                    agg.total_principal_delivered += r.recipient_amount;
                    agg.total_sadaqah_tips += r.sadaqah_tip_amount;
                    agg.total_fees_deducted += r.protocol_fee_deducted;
                }
            }
            agg
        }
    }

    pub fn compute_split_amounts(gross_amount_cents: u64) -> (u64, u64, u64) {
        let lender_amount_cents = (gross_amount_cents * 9300) / 10000;
        let shield_amount_cents = (gross_amount_cents * 400) / 10000;
        let saas_amount_cents = gross_amount_cents - (lender_amount_cents + shield_amount_cents);
        (lender_amount_cents, shield_amount_cents, saas_amount_cents)
    }

    /// Computes and validates corridor routing splits.
    ///
    /// Panics if `mode == CorridorSplitMode::ZAKAT_TAMLIK_100` and `protocol_fee_deduction > 0`.
    pub fn compute_corridor_split(
        mode: CorridorSplitMode,
        zakat_principal_amount: u64,
        sadaqah_tip_amount: u64,
        protocol_fee_deduction: u64,
    ) -> (u64, u64, u64) {
        assert!(
            zakat_principal_amount > 0,
            "Principal amount must be strictly positive"
        );

        match mode {
            CorridorSplitMode::ZAKAT_TAMLIK_100 => {
                assert!(
                    protocol_fee_deduction == 0,
                    "Tamlik Violation: ZAKAT_TAMLIK_100 strictly forbids deducting any protocol fee, gas, or FX spread ({}) from zakatPrincipalAmount ({})",
                    protocol_fee_deduction,
                    zakat_principal_amount
                );
                // 100% direct ownership transfer (Tamlik) to recipientBridgeVault; optional separate sadaqahTipAmount
                (zakat_principal_amount, sadaqah_tip_amount, 0)
            }
            CorridorSplitMode::COMMERCIAL_93_4_3 => {
                let (owner_93, shield_4, saas_3) = compute_split_amounts(zakat_principal_amount);
                let total_protocol_fee = shield_4 + saas_3;
                (owner_93, sadaqah_tip_amount, total_protocol_fee)
            }
        }
    }

    pub fn validate_zakat_remittance_split(
        mode: CorridorSplitMode,
        corridor_jurisdiction: &str,
        recipient_bridge_vault: &str,
        zakat_principal_amount: u64,
        sadaqah_tip_amount: u64,
        protocol_fee_deduction: u64,
        recipient_commitment_hash: &Checksum256,
    ) -> (u64, u64, u64) {
        assert!(
            *recipient_commitment_hash != Checksum256::from([0u8; 32]),
            "PII Guard Violation: recipientCommitmentHash must be a non-zero 32-byte salted SHA-256 digest"
        );
        assert!(
            corridor_jurisdiction.len() == 2
                && corridor_jurisdiction.chars().all(|c| c.is_ascii_uppercase()),
            "corridorJurisdiction '{}' must be a 2-letter uppercase ISO-3166-1 alpha-2 bridge code (e.g. TR, AE, UA, UZ)",
            corridor_jurisdiction
        );
        assert!(
            !recipient_bridge_vault.trim().is_empty(),
            "recipientBridgeVault must be a non-empty non-custodial vault or licensed bridge settlement address"
        );

        compute_corridor_split(
            mode,
            zakat_principal_amount,
            sadaqah_tip_amount,
            protocol_fee_deduction,
        )
    }

    pub fn build_remittance_multisig_payload(
        mode: CorridorSplitMode,
        rail_type: &str,
        corridor_jurisdiction: &str,
        recipient_bridge_vault: &str,
        infra_ops_vault: Option<&str>,
        recipient_amount: u64,
        sadaqah_tip_amount: u64,
        protocol_fee_deducted: u64,
        attestation_id: u64,
        recipient_commitment_hash: &Checksum256,
    ) -> String {
        let mut transfers = vec![serde_json::json!({
            "leg": "zakat_tamlik_principal",
            "to": recipient_bridge_vault,
            "amount": recipient_amount,
            "fee_deducted": protocol_fee_deducted
        })];

        if sadaqah_tip_amount > 0 {
            transfers.push(serde_json::json!({
                "leg": "sadaqah_infra_tip",
                "to": infra_ops_vault.unwrap_or("charity-sink"),
                "amount": sadaqah_tip_amount
            }));
        }

        serde_json::json!({
            "custody": "strict_non_custodial_23_nycrr_200_2_q",
            "mode": mode.as_str(),
            "rail_type": rail_type,
            "corridor_jurisdiction": corridor_jurisdiction,
            "attestation_id": attestation_id,
            "recipient_commitment_hash": recipient_commitment_hash.to_string(),
            "total_debited": recipient_amount + sadaqah_tip_amount + protocol_fee_deducted,
            "transfers": transfers
        })
        .to_string()
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

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    #[psibase::test_case(packages("ExternalVault"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::service::{
        build_remittance_multisig_payload, compute_corridor_split, compute_split_amounts,
        validate_zakat_remittance_split,
    };
    use crate::tables::CorridorSplitMode;
    use psibase::fracpack::{Pack, Unpack};
    use psibase::Checksum256;

    #[test]
    fn test_split_amounts_exact_conservation() {
        let (lender, shield, saas) = compute_split_amounts(10_000);
        assert_eq!((lender, shield, saas), (9_300, 400, 300));
        assert_eq!(lender + shield + saas, 10_000);

        // Prime/unaligned cent value conserves exact total without rounding loss
        let (l, sh, s) = compute_split_amounts(1_001);
        assert_eq!(l + sh + s, 1_001);
    }

    #[test]
    fn test_zakat_tamlik_100_routes_100_percent_principal_and_separate_sadaqah_tip() {
        let commitment = Checksum256::from([0x7a; 32]);
        let (recipient, tip, fee) = validate_zakat_remittance_split(
            CorridorSplitMode::ZAKAT_TAMLIK_100,
            "TR",
            "0xSafeIstanbulBridgeVault000000000000000001",
            100_000, // $1,000.00 Zakat principal
            2_500,   // $25.00 optional separate Sadaqah tip for FX/ops
            0,       // 0 fee deducted from Zakat principal
            &commitment,
        );

        assert_eq!(recipient, 100_000);
        assert_eq!(tip, 2_500);
        assert_eq!(fee, 0);

        let payload = build_remittance_multisig_payload(
            CorridorSplitMode::ZAKAT_TAMLIK_100,
            "evm_safe",
            "TR",
            "0xSafeIstanbulBridgeVault000000000000000001",
            Some("0xQirimReliefOpsCharitySink00000000000002"),
            recipient,
            tip,
            fee,
            42,
            &commitment,
        );
        assert!(payload.contains("\"mode\":\"ZAKAT_TAMLIK_100\""));
        assert!(payload.contains("\"total_debited\":102500"));
        assert!(payload.contains("\"leg\":\"sadaqah_infra_tip\""));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "Tamlik Violation: ZAKAT_TAMLIK_100 strictly forbids deducting any protocol fee, gas, or FX spread (1) from zakatPrincipalAmount (50000)"
    )]
    fn test_zakat_tamlik_100_panics_on_any_nonzero_fee_deduction_from_principal() {
        compute_corridor_split(
            CorridorSplitMode::ZAKAT_TAMLIK_100,
            50_000,
            500,
            1, // Even 1 cent / 1 satoshi of fee deduction must panic!
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "PII Guard Violation: recipientCommitmentHash must be a non-zero 32-byte salted SHA-256 digest"
    )]
    fn test_zakat_remittance_rejects_zero_commitment_hash() {
        validate_zakat_remittance_split(
            CorridorSplitMode::ZAKAT_TAMLIK_100,
            "UA",
            "bc1pbridgevaulttaproot0000000000000000000000000",
            25_000,
            0,
            0,
            &Checksum256::from([0u8; 32]),
        );
    }

    #[test]
    fn test_corridor_split_mode_fracpack_roundtrip_and_commercial_default() {
        assert_eq!(
            CorridorSplitMode::default(),
            CorridorSplitMode::COMMERCIAL_93_4_3
        );
        for mode in [
            CorridorSplitMode::COMMERCIAL_93_4_3,
            CorridorSplitMode::ZAKAT_TAMLIK_100,
        ] {
            let bytes = mode.packed();
            let decoded = CorridorSplitMode::unpacked(&bytes).unwrap();
            assert_eq!(decoded, mode);
        }
    }

    #[test]
    fn given_unsettled_zakat_intent_when_donor_confirms_then_deterministic_receipt_is_built() {
        use crate::service::{build_corridor_receipt, validate_settlement_caller};
        use crate::tables::ZakatRemittanceIntent;
        use psibase::account;

        let donor = account!("donor");
        let svc = account!("ext-vault");
        validate_settlement_caller(donor, donor, svc, false);
        validate_settlement_caller(svc, donor, svc, false);

        let intent = ZakatRemittanceIntent {
            intent_id: 9,
            attestation_id: 3,
            donor,
            mode: CorridorSplitMode::ZAKAT_TAMLIK_100,
            rail_type: "evm_safe".into(),
            corridor_jurisdiction: "TR".into(),
            recipient_bridge_vault: "0xSafeBridge".into(),
            infra_ops_vault: Some("0xOpsSink".into()),
            zakat_principal_amount: 100_000,
            recipient_amount: 100_000,
            sadaqah_tip_amount: 1_500,
            protocol_fee_deducted: 0,
            recipient_commitment_hash: Checksum256::from([0x7a; 32]),
            payload_json: "{}".into(),
            external_tx_ref: None,
            settled: false,
        };
        let r1 = build_corridor_receipt(&intent, "0xtxhash");
        let r2 = build_corridor_receipt(&intent, "0xtxhash");
        assert_eq!(r1.receipt_leaf_hash, r2.receipt_leaf_hash);
        assert_ne!(r1.receipt_leaf_hash, Checksum256::from([0u8; 32]));
        assert_eq!(r1.protocol_fee_deducted, 0);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Unauthorized settlement confirmation")]
    fn given_zakat_intent_when_unrelated_caller_confirms_then_rejected() {
        use crate::service::validate_settlement_caller;
        use psibase::account;
        validate_settlement_caller(account!("intruder"), account!("donor"), account!("ext-vault"), false);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "already been settled")]
    fn given_settled_zakat_intent_when_reconfirmed_then_rejected() {
        use crate::service::validate_settlement_caller;
        use psibase::account;
        validate_settlement_caller(account!("donor"), account!("donor"), account!("ext-vault"), true);
    }
}
