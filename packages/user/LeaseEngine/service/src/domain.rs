//! Pure UsufructLease validation, commercial split conservation, and Anti-Riba late-return rules.

use crate::tables::tables::{UsufructLease, STATUS_ACTIVE, STATUS_CREATED};
use muabase_institutional::{
    compute_commercial_split, validate_non_zero_digest, CommercialSplit,
};
use psibase::{AccountNumber, Checksum256};

pub const DEFAULT_PROTOCOL_SAAS_FEE_BPS: u16 = 300;
pub const DEFAULT_MUTUAL_SHIELD_FEE_BPS: u16 = 400;

pub fn validate_lease_creation(
    lessee: AccountNumber,
    lessor: AccountNumber,
    duration_sec: u64,
    gross_payment: u64,
    pre_condition_evidence_hash: &Checksum256,
    custodian_account: AccountNumber,
    custodian_fee_bps: u16,
    charity_sink_account: AccountNumber,
) -> CommercialSplit {
    assert_ne!(lessee, lessor, "Lessee and lessor must be distinct");
    assert!(gross_payment > 0, "Gross payment must be positive");
    assert!(duration_sec > 0, "Duration must be positive");
    assert_ne!(
        charity_sink_account, lessor,
        "Riba Violation: Charity sink cannot be the lessor"
    );
    validate_non_zero_digest(&pre_condition_evidence_hash.0, "pre_condition_evidence_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    if custodian_fee_bps > 0 {
        assert_ne!(
            custodian_account.value, 0,
            "custodian_account must be set when custodian_fee_bps > 0"
        );
    }
    compute_commercial_split(
        gross_payment,
        DEFAULT_PROTOCOL_SAAS_FEE_BPS,
        DEFAULT_MUTUAL_SHIELD_FEE_BPS,
        custodian_fee_bps,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

pub fn validate_settlement_and_compute_split(
    lease: &UsufructLease,
    caller: AccountNumber,
    service_acct: AccountNumber,
) -> CommercialSplit {
    assert!(
        caller == service_acct || caller == lease.lessee,
        "Only lease-eng or lessee may settle lease payment"
    );
    assert_eq!(
        lease.status, STATUS_CREATED,
        "Lease payment already settled or not in CREATED state"
    );
    if lease.custodian_fee_bps > 0 {
        assert_ne!(
            lease.custodian_account.value, 0,
            "custodian_account must be set when custodian_fee_bps > 0"
        );
    }
    compute_commercial_split(
        lease.gross_payment.value,
        lease.protocol_saas_fee_bps,
        lease.mutual_shield_fee_bps,
        lease.custodian_fee_bps,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

pub fn validate_return_finalization(
    lease: &UsufructLease,
    caller: AccountNumber,
    service_acct: AccountNumber,
    post_condition_evidence_hash: &Checksum256,
) {
    assert!(
        caller == lease.lessor || caller == service_acct,
        "Only lessor or lease-eng may finalize lease return"
    );
    assert_eq!(lease.status, STATUS_ACTIVE, "Lease not active");
    validate_non_zero_digest(
        &post_condition_evidence_hash.0,
        "post_condition_evidence_hash",
    )
    .unwrap_or_else(|e| panic!("{e}"));
}

pub fn validate_late_return(
    lease: &UsufructLease,
    caller: AccountNumber,
    service_acct: AccountNumber,
    post_condition_evidence_hash: &Checksum256,
) {
    validate_return_finalization(lease, caller, service_acct, post_condition_evidence_hash);
    assert_ne!(
        lease.charity_sink_account, lease.lessor,
        "Riba Violation: Creditor/lessor can never receive late penalties"
    );
}
