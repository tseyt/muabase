#[psibase::service_tables]
#[allow(clippy::module_inception)]
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
        pub btc_taproot_descriptor: String,
        pub evm_safe_address: String,
        pub solana_squads_pda: String,
        pub stripe_connect_account: String,
        pub charity_sink_stripe: String,
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
        pub rail_type: String,
        pub gross_amount_cents: u64,
        pub lender_amount_cents: u64,
        pub shield_amount_cents: u64,
        pub saas_amount_cents: u64,
        pub payload_json: String,
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
        pub rail_type: String,
        pub corridor_jurisdiction: String,
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
