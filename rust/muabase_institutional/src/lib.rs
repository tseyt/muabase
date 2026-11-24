//! Shared institutional primitives reused across Muabase domain services:
//! - Cryptographic digests, hex codecs, and `dwal_merkle`-compatible leaf hashing (`codec`)
//! - Zero-PII commitment guards, ISO corridor validation, and overflow-safe commercial splits (`validation`)
//! - Double-entry append-only ledger conservation & audit receipt digests (`ledger`)
//! - Scholar/auditor `ReviewerAttestation` & deterministic compliance policy hooks (`compliance`)
//! - Pluggable 2-of-3 `TahkimPanel` dispute resolution (`tahkim`)
//! - Salted SHA-256 commitment + view-key selective-disclosure encrypted envelopes (`privacy`)

pub mod codec;
pub mod compliance;
pub mod ledger;
pub mod privacy;
pub mod tahkim;
pub mod validation;

pub use codec::{
    dwal_merkle_leaf_hash, from_hex, from_hex_fixed, sha256_bytes, to_hex, DWAL_LEAF_DOMAIN_TAG,
    DWAL_VALUE_TAG_BYTES,
};
pub use compliance::{CompliancePolicy, ReviewerAttestation};
pub use ledger::{JournalEntry, JournalLine};
pub use privacy::{
    compute_salted_commitment, SelectiveDisclosureEnvelope, ViewKeyGrant, PRIVACY_ENVELOPE_VERSION,
};
pub use tahkim::{TahkimPanel, TahkimRuling, TahkimVote};
pub use validation::{
    compute_commercial_split, parse_non_zero_digest_hex, validate_iso_country_code,
    validate_non_zero_digest, CommercialSplit, BPS_DENOMINATOR,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_hex_and_digest_guards_when_validated_then_accepts_canonical_and_rejects_malformed_or_zero() {
        let digest = sha256_bytes(b"beneficiary-packet-1");
        let hex = to_hex(&digest);
        assert_eq!(parse_non_zero_digest_hex(&hex, "commitment").unwrap(), digest);
        assert_eq!(
            parse_non_zero_digest_hex(&format!("0x{hex}"), "commitment").unwrap(),
            digest
        );
        assert_eq!(
            parse_non_zero_digest_hex(&format!("0X{}", hex.to_uppercase()), "commitment").unwrap(),
            digest
        );

        assert!(validate_non_zero_digest(&[0u8; 32], "commitment").is_err());
        assert!(parse_non_zero_digest_hex(&"0".repeat(64), "commitment").is_err());
        assert!(parse_non_zero_digest_hex("0x123", "commitment").is_err());
        assert!(parse_non_zero_digest_hex("Amina Qirimli", "commitment").is_err());

        assert!(validate_iso_country_code("TR", "corridor").is_ok());
        assert!(validate_iso_country_code("tr", "corridor").is_err());
        assert!(validate_iso_country_code("TUR", "corridor").is_err());
        assert!(validate_iso_country_code("T1", "corridor").is_err());
    }

    #[test]
    fn given_commercial_gross_amount_when_split_computed_then_conserves_exact_total_without_u64_overflow() {
        let split = compute_commercial_split(1_001, 300, 400, 0).unwrap();
        assert_eq!(split.saas_amount, 30);
        assert_eq!(split.shield_amount, 40);
        assert_eq!(split.custodian_amount, 0);
        assert_eq!(split.net_owner_amount, 931);
        assert_eq!(
            split.net_owner_amount + split.shield_amount + split.saas_amount + split.custodian_amount,
            1_001
        );

        // Overflow-safe even at u64::MAX
        let huge = compute_commercial_split(u64::MAX, 300, 400, 100).unwrap();
        assert_eq!(
            (huge.net_owner_amount as u128)
                + (huge.shield_amount as u128)
                + (huge.saas_amount as u128)
                + (huge.custodian_amount as u128),
            u64::MAX as u128
        );

        assert!(compute_commercial_split(0, 300, 400, 0).is_err());
        assert!(compute_commercial_split(10_000, 5_000, 5_000, 0).is_err());
    }

    #[test]
    fn given_double_entry_journal_when_balanced_then_conserves_and_emits_dwal_receipt_hash() {
        let entry = JournalEntry::transfer(
            7,
            "zakat_fuqara",
            "corridor-pool",
            "bridge-tr",
            100_000,
            [0x11; 32],
        )
        .unwrap();
        assert!(entry.validate_balanced().is_ok());
        let h1 = entry.receipt_leaf_hash("receipt/corridor").unwrap();
        let h2 = entry.receipt_leaf_hash("receipt/corridor").unwrap();
        assert_eq!(h1, h2);
        assert_ne!(h1, [0u8; 32]);

        // Fan-out omits zero legs and rejects zero commitment or overflow
        let fan = JournalEntry::fan_out(
            9,
            "corridor:TR",
            "donor:ali",
            &[("bridge:0x1".into(), 100_000), ("ops:0x2".into(), 0)],
            [0x22; 32],
        )
        .unwrap();
        assert_eq!(fan.lines.len(), 2);
        assert!(JournalEntry::transfer(10, "x", "a", "b", 100, [0u8; 32]).is_err());
        assert!(JournalEntry::transfer(11, "x", "a", "b", u64::MAX, [0x11; 32]).is_err());

        let unbalanced = JournalEntry {
            entry_id: 8,
            category: "zakat_fuqara".into(),
            lines: vec![
                JournalLine { account: "corridor-pool".into(), delta: -100_000 },
                JournalLine { account: "bridge-tr".into(), delta: 99_000 },
            ],
            commitment_hash: [0x11; 32],
        };
        assert!(unbalanced.validate_balanced().is_err());
    }

    #[test]
    fn given_compliance_policy_and_scholar_attestation_when_enforced_then_violations_fail_deterministically() {
        let tpl = sha256_bytes(b"istisna-milestone-v1");
        let att = ReviewerAttestation {
            reviewer_account: "shariah-brd".into(),
            template_hash: tpl,
            certificate_hash: sha256_bytes(b"aaoifi-cert-2026"),
            standard_ref: "AAOIFI-11".into(),
            revoked: false,
        };
        assert!(att.validate_active(&tpl).is_ok());
        assert!(att.validate_active(&[0xaa; 32]).is_err());

        let mut revoked_att = att.clone();
        revoked_att.revoked = true;
        assert!(revoked_att.validate_active(&tpl).is_err());

        let mut empty_ref_att = att;
        empty_ref_att.standard_ref = "   ".into();
        assert!(empty_ref_att.validate_active(&tpl).is_err());

        let policy = CompliancePolicy {
            max_single_disbursement: 500_000,
            allowed_categories: vec!["scholarship".into(), "utility".into()],
            allowlist_recipients: vec!["masjid-ops".into()],
            require_zero_fee_tamlik: true,
        };
        assert!(policy.validate_disbursement("masjid-ops", "scholarship", 250_000, 0).is_ok());
        assert!(policy.validate_disbursement("masjid-ops", "scholarship", 250_000, 1).is_err());
        assert!(policy.validate_disbursement("masjid-ops", "unapproved", 10_000, 0).is_err());
        assert!(policy.validate_disbursement("masjid-ops", "utility", 600_000, 0).is_err());
        assert!(policy.validate_disbursement("outsider", "utility", 10_000, 0).is_err());
    }

    #[test]
    fn given_tahkim_panel_when_two_of_three_agree_then_ruling_resolves_and_rejects_overflow_or_post_quorum_votes() {
        assert!(TahkimPanel::new(["arb1".into(), "arb1".into(), "arb3".into()]).is_err());

        let mut panel = TahkimPanel::new(["arb1".into(), "arb2".into(), "arb3".into()]).unwrap();
        // Overflowing split must fail even when total_escrow == u64::MAX
        let overflowing_split = TahkimRuling::Split {
            funder_amount: u64::MAX,
            counterparty_amount: 1,
        };
        assert!(panel.cast_vote("arb1", overflowing_split, u64::MAX).is_err());

        let split = TahkimRuling::Split {
            funder_amount: 40_000,
            counterparty_amount: 60_000,
        };
        assert_eq!(panel.cast_vote("arb1", split.clone(), 100_000).unwrap(), None);
        assert!(panel.cast_vote("arb1", split.clone(), 100_000).is_err());
        assert!(panel.cast_vote("outsider", split.clone(), 100_000).is_err());
        assert_eq!(
            panel.cast_vote("arb2", split.clone(), 100_000).unwrap(),
            Some(split.clone())
        );
        // Post-quorum third vote must be rejected
        assert!(panel.cast_vote("arb3", split, 100_000).is_err());
    }

    #[test]
    fn given_selective_disclosure_envelope_when_opened_with_view_key_then_pii_matches_commitment_and_tampering_fails() {
        let pii = br#"{"beneficiary":"Amina Q.","iban":"TR0012345"}"#;
        let salt = [0x42u8; 32];
        let content_key = [0x77u8; 32];
        let nonce = [0x99u8; 16];
        let auditor_secret = [0x12u8; 32];
        let wrong_secret = [0x34u8; 32];

        let env = SelectiveDisclosureEnvelope::seal(
            pii,
            &salt,
            &content_key,
            &nonce,
            &[("auditor-1", auditor_secret)],
        );
        let serialized = serde_json::to_string(&env).unwrap();
        assert!(!serialized.contains("Amina"));
        assert!(!serialized.contains("TR0012345"));

        let opened = env.open_for_viewer("auditor-1", &auditor_secret).unwrap();
        assert_eq!(opened, pii);

        assert!(env.open_for_viewer("auditor-1", &wrong_secret).is_err());
        assert!(env.open_for_viewer("random-user", &auditor_secret).is_err());

        let mut tampered_ct = env.clone();
        tampered_ct.ciphertext_hex.replace_range(0..2, "ff");
        assert!(tampered_ct.open_for_viewer("auditor-1", &auditor_secret).is_err());

        let mut bad_ver = env;
        bad_ver.version = "muabase.privacy.v99".into();
        assert!(bad_ver.open_for_viewer("auditor-1", &auditor_secret).is_err());
    }
}
