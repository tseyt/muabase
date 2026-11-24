//! Shared non-leaky domain validation guards and conservation-preserving split arithmetic.

use crate::codec::from_hex_fixed;
use serde::{Deserialize, Serialize};

/// Basis points denominator (`10_000` bps = 100.00%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Rejects uninitialized all-zero 32-byte SHA-256 digests (zero-PII commitment guard).
pub fn validate_non_zero_digest(digest: &[u8; 32], field_name: &str) -> Result<(), String> {
    if *digest == [0u8; 32] {
        return Err(format!(
            "PII Guard Violation: {field_name} must be a non-zero 32-byte SHA-256 digest"
        ));
    }
    Ok(())
}

/// Parses an optional `0x`/`0X`-prefixed 32-byte SHA-256 hex string and rejects all-zero digests.
pub fn parse_non_zero_digest_hex(hex: &str, field_name: &str) -> Result<[u8; 32], String> {
    let digest = from_hex_fixed::<32>(hex, field_name).map_err(|_| {
        format!(
            "PII Guard Violation: {field_name} must be a 32-byte (64-hex-char) SHA-256 digest"
        )
    })?;
    validate_non_zero_digest(&digest, field_name)?;
    Ok(digest)
}

/// Enforces uppercase ISO-3166-1 alpha-2 bridge jurisdiction codes (e.g., `"TR"`, `"AE"`, `"UA"`).
pub fn validate_iso_country_code(code: &str, field_name: &str) -> Result<(), String> {
    if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(format!(
            "PII & Compliance Violation: {field_name} '{code}' must be a 2-letter uppercase ISO-3166-1 alpha-2 code (e.g., TR, AE, UA, UZ)"
        ));
    }
    Ok(())
}

/// Conservation-preserving commercial split breakdown (rounding remainder accrues to `net_owner_amount`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialSplit {
    pub net_owner_amount: u64,
    pub shield_amount: u64,
    pub saas_amount: u64,
    pub custodian_amount: u64,
}

/// Computes an overflow-safe `u128` multi-party commercial split (e.g. 93% owner / 4% shield / 3% SaaS).
/// Any integer division truncation remainder accrues to `net_owner_amount`, guaranteeing exact conservation.
pub fn compute_commercial_split(
    gross_amount: u64,
    saas_bps: u16,
    shield_bps: u16,
    custodian_bps: u16,
) -> Result<CommercialSplit, String> {
    if gross_amount == 0 {
        return Err("Gross amount must be positive".into());
    }
    let total_fee_bps = saas_bps as u128 + shield_bps as u128 + custodian_bps as u128;
    if total_fee_bps >= BPS_DENOMINATOR {
        return Err(format!(
            "Total fee basis points ({total_fee_bps}) must be strictly less than {BPS_DENOMINATOR}"
        ));
    }
    let gross = gross_amount as u128;
    let saas_amount = (gross * saas_bps as u128 / BPS_DENOMINATOR) as u64;
    let shield_amount = (gross * shield_bps as u128 / BPS_DENOMINATOR) as u64;
    let custodian_amount = (gross * custodian_bps as u128 / BPS_DENOMINATOR) as u64;
    let total_deductions = saas_amount
        .checked_add(shield_amount)
        .and_then(|v| v.checked_add(custodian_amount))
        .ok_or_else(|| "Commercial split deduction overflow".to_string())?;
    let net_owner_amount = gross_amount
        .checked_sub(total_deductions)
        .ok_or_else(|| "Commercial split deductions exceed gross amount".to_string())?;

    Ok(CommercialSplit {
        net_owner_amount,
        shield_amount,
        saas_amount,
        custodian_amount,
    })
}
