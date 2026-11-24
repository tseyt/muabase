#![allow(non_snake_case)]

pub mod domain;
mod tables;
#[cfg(test)]
mod tests;

pub use tables::tables as external_vault_tables;

#[allow(clippy::too_many_arguments)]
#[psibase::service(name = "ext-vault", tables = "tables::tables")]
pub mod service {
    pub use crate::domain::{
        build_commercial_split_payload, build_corridor_receipt, build_remittance_multisig_payload,
        compute_corridor_split, compute_split_amounts, validate_rail_type,
        validate_settlement_caller, validate_zakat_remittance_split, DEFAULT_CHARITY_SINK_VAULT,
    };
    use crate::tables::tables::*;
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
        add_index("complianceReceipt", 0);
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
        validate_rail_type(&rail_type);

        let (lender_amount_cents, shield_amount_cents, saas_amount_cents) =
            compute_split_amounts(gross_amount_cents);

        let lender_config = VaultConfigTable::read().get_index_pk().get(&lender_account);
        let stripe_acct = lender_config
            .as_ref()
            .map(|c| c.stripe_connect_account.as_str())
            .unwrap_or("acct_unregistered");

        let payload_json = build_commercial_split_payload(
            lease_id,
            &rail_type,
            gross_amount_cents,
            lender_account,
            stripe_acct,
            lender_amount_cents,
            shield_amount_cents,
            saas_amount_cents,
        );

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
        Wrapper::emit()
            .history()
            .intentGenerated(intent_id, lease_id, payer, gross_amount_cents);

        intent_id
    }

    /// Confirms that external non-custodial transaction has settled on-chain or via Stripe
    #[action]
    pub fn confirmSettlement(intent_id: u64, external_tx_ref: String) {
        assert!(
            !external_tx_ref.trim().is_empty(),
            "external_tx_ref settlement confirmation hash must be non-empty"
        );
        let mut intent = SplitIntentTable::read()
            .get_index_pk()
            .get(&intent_id)
            .expect("Intent DNE");

        validate_settlement_caller(get_sender(), intent.payer, SERVICE, intent.settled);

        intent.external_tx_ref = Some(external_tx_ref.clone());
        intent.settled = true;

        SplitIntentTable::new().put(&intent).unwrap();
        Wrapper::emit()
            .history()
            .intentSettled(intent_id, intent.lease_id, external_tx_ref);
    }

    /// Generates a non-custodial Zakat *Tamlik* or Humanitarian Corridor settlement split intent.
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
        validate_rail_type(&rail_type);

        let (recipient_amount, tip_routed_amount, fee_deducted) = validate_zakat_remittance_split(
            mode,
            &corridor_jurisdiction,
            &recipient_bridge_vault,
            zakat_principal_amount,
            sadaqah_tip_amount,
            protocol_fee_deduction,
            &recipient_commitment_hash,
        );

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
                    .unwrap_or_else(|| DEFAULT_CHARITY_SINK_VAULT.to_string()),
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

    /// Confirms external non-custodial execution of a Zakat / Humanitarian remittance split.
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
        Wrapper::emit()
            .history()
            .remitSettled(intent_id, intent.attestation_id, external_tx_ref);
        Wrapper::emit()
            .history()
            .complianceReceipt(intent_id, receipt.receipt_leaf_hash);
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
                    agg.settled_count = agg.settled_count.saturating_add(1);
                    agg.total_principal_delivered = agg
                        .total_principal_delivered
                        .saturating_add(r.recipient_amount);
                    agg.total_sadaqah_tips =
                        agg.total_sadaqah_tips.saturating_add(r.sadaqah_tip_amount);
                    agg.total_fees_deducted = agg
                        .total_fees_deducted
                        .saturating_add(r.protocol_fee_deducted);
                }
            }
            agg
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
