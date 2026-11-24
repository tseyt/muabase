//! Pure non-custodial commercial/corridor split math, payload builders, and compliance receipts.

use crate::tables::tables::{CorridorReceipt, CorridorSplitMode, ZakatRemittanceIntent};
use muabase_institutional::{
    compute_commercial_split, validate_iso_country_code, validate_non_zero_digest, JournalEntry,
};
use psibase::{AccountNumber, Checksum256};

pub const COMMERCIAL_SAAS_FEE_BPS: u16 = 300;
pub const COMMERCIAL_SHIELD_FEE_BPS: u16 = 400;
pub const CANONICAL_MUTUAL_SHIELD_ACCOUNT: &str = "mut-shield";
pub const CANONICAL_SAAS_ACCOUNT: &str = "psi-saas";
pub const DEFAULT_CHARITY_SINK_VAULT: &str = "charity-sink";
pub const RECEIPT_KEY_PREFIX: &str = "receipt/corridor";

pub fn validate_settlement_caller(
    sender: AccountNumber,
    owner: AccountNumber,
    service_acct: AccountNumber,
    already_settled: bool,
) {
    assert!(!already_settled, "Intent has already been settled");
    assert!(
        sender == owner || sender == service_acct,
        "Unauthorized settlement confirmation: caller '{sender}' must be '{owner}' or '{service_acct}'"
    );
}

pub fn validate_rail_type(rail_type: &str) {
    assert!(
        !rail_type.trim().is_empty(),
        "rail_type must be non-empty"
    );
}

pub fn compute_split_amounts(gross_amount_cents: u64) -> (u64, u64, u64) {
    let split = compute_commercial_split(
        gross_amount_cents,
        COMMERCIAL_SAAS_FEE_BPS,
        COMMERCIAL_SHIELD_FEE_BPS,
        0,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    (split.net_owner_amount, split.shield_amount, split.saas_amount)
}

pub fn build_commercial_split_payload(
    lease_id: u64,
    rail_type: &str,
    gross_amount_cents: u64,
    lender_account: AccountNumber,
    stripe_acct: &str,
    lender_amount_cents: u64,
    shield_amount_cents: u64,
    saas_amount_cents: u64,
) -> String {
    match rail_type {
        "stripe_connect" => serde_json::json!({
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
        })
        .to_string(),
        "bitcoin_taproot" => serde_json::json!({
            "psbt_type": "miniscript_p2tr_3_of_5",
            "total_sats": gross_amount_cents,
            "outputs": [
                { "recipient": "lender_p2tr", "amount_sats": lender_amount_cents },
                { "recipient": "mutual_shield_p2tr", "amount_sats": shield_amount_cents },
                { "recipient": "saas_host_p2tr", "amount_sats": saas_amount_cents }
            ]
        })
        .to_string(),
        "evm_safe" => serde_json::json!({
            "safe_multisend": true,
            "token": "USDC_OR_PAXG",
            "transfers": [
                { "to": lender_account.to_string(), "amount": lender_amount_cents },
                { "to": CANONICAL_MUTUAL_SHIELD_ACCOUNT, "amount": shield_amount_cents },
                { "to": CANONICAL_SAAS_ACCOUNT, "amount": saas_amount_cents }
            ]
        })
        .to_string(),
        _ => serde_json::json!({
            "standard": "generic_non_custodial_split",
            "gross": gross_amount_cents,
            "splits": {
                "lender": lender_amount_cents,
                "shield": shield_amount_cents,
                "saas": saas_amount_cents
            }
        })
        .to_string(),
    }
}

/// Computes and validates corridor routing splits.
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
                "Tamlik Violation: ZAKAT_TAMLIK_100 strictly forbids deducting any protocol fee, gas, or FX spread ({protocol_fee_deduction}) from zakatPrincipalAmount ({zakat_principal_amount})"
            );
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
    validate_non_zero_digest(&recipient_commitment_hash.0, "recipient_commitment_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    validate_iso_country_code(corridor_jurisdiction, "corridor_jurisdiction")
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        !recipient_bridge_vault.trim().is_empty(),
        "recipient_bridge_vault must be a non-empty non-custodial vault or licensed bridge settlement address"
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
    let total_debited = recipient_amount
        .checked_add(sadaqah_tip_amount)
        .and_then(|v| v.checked_add(protocol_fee_deducted))
        .expect("Remittance total_debited overflow");

    let mut transfers = vec![serde_json::json!({
        "leg": "zakat_tamlik_principal",
        "to": recipient_bridge_vault,
        "amount": recipient_amount,
        "fee_deducted": protocol_fee_deducted
    })];

    if sadaqah_tip_amount > 0 {
        transfers.push(serde_json::json!({
            "leg": "sadaqah_infra_tip",
            "to": infra_ops_vault.unwrap_or(DEFAULT_CHARITY_SINK_VAULT),
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
        "total_debited": total_debited,
        "transfers": transfers
    })
    .to_string()
}

pub fn build_corridor_receipt(
    intent: &ZakatRemittanceIntent,
    external_tx_ref: &str,
) -> CorridorReceipt {
    let ops_account = format!(
        "ops:{}",
        intent
            .infra_ops_vault
            .as_deref()
            .unwrap_or(DEFAULT_CHARITY_SINK_VAULT)
    );
    let entry = JournalEntry::fan_out(
        intent.intent_id,
        format!("corridor:{}", intent.corridor_jurisdiction),
        format!("donor:{}", intent.donor),
        &[
            (
                format!("bridge:{}", intent.recipient_bridge_vault),
                intent.recipient_amount,
            ),
            (ops_account, intent.sadaqah_tip_amount),
            ("protocol:fee".to_string(), intent.protocol_fee_deducted),
        ],
        intent.recipient_commitment_hash.0,
    )
    .expect("Double-entry corridor conservation violated");

    let leaf = entry
        .receipt_leaf_hash(RECEIPT_KEY_PREFIX)
        .expect("Double-entry corridor receipt digest failed");

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
