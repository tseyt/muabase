#[cfg(all(test, target_family = "wasm"))]
mod tests {
    #[psibase::test_case(packages("Credentials"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::service::{
        apply_precision_score_update, apply_trust_score_update, validate_domain_badge_issuance,
        validate_trust_score_update, DEFAULT_INITIAL_PRECISION_SCORE,
    };
    use crate::tables::tables::DomainBadgeRecord;
    use psibase::{account, Checksum256, TimePointSec};

    #[test]
    fn test_new_account_trust_score_initializes_from_zero_and_tracks_audits() {
        let alice = account!("alice");
        let auditor = account!("auditor");
        validate_trust_score_update(auditor, alice, 8500);

        let rec1 = apply_trust_score_update(None, alice, 8500, true, TimePointSec { seconds: 100 });
        assert_eq!(rec1.score, 8500);
        assert_eq!(rec1.total_audits, 1);
        assert_eq!(rec1.successful_audits, 1);
        assert_eq!(rec1.disputes_involved, 0);

        let rec2 = apply_trust_score_update(
            Some(rec1),
            alice,
            8000,
            false,
            TimePointSec { seconds: 200 },
        );
        assert_eq!(rec2.score, 8000);
        assert_eq!(rec2.total_audits, 2);
        assert_eq!(rec2.successful_audits, 1);
        assert_eq!(rec2.disputes_involved, 1);
    }

    #[test]
    #[should_panic(expected = "Cannot report on your own trust score")]
    fn test_self_reporting_trust_score_is_rejected() {
        let alice = account!("alice");
        validate_trust_score_update(alice, alice, 9900);
    }

    #[test]
    #[should_panic(expected = "trust_score must be within 0..=10000 bps")]
    fn test_out_of_bounds_trust_score_is_rejected() {
        let alice = account!("alice");
        let auditor = account!("auditor");
        validate_trust_score_update(auditor, alice, 10001);
    }

    #[test]
    #[should_panic(expected = "Cannot issue a domain badge to yourself")]
    fn test_self_issued_domain_badge_is_rejected() {
        let alice = account!("alice");
        validate_domain_badge_issuance(alice, alice, "master_appraiser");
    }

    #[test]
    #[should_panic(expected = "Domain must not be empty")]
    fn test_empty_domain_badge_is_rejected() {
        let alice = account!("alice");
        let guild = account!("guild");
        validate_domain_badge_issuance(guild, alice, "");
    }

    #[test]
    fn test_domain_badge_precision_score_tracks_accurate_deliveries_and_regressions() {
        let alice = account!("alice");
        let verifier = account!("verifier");
        let badge = DomainBadgeRecord {
            account: alice,
            domain: "rust_consensus".to_string(),
            issuer: verifier,
            issued_at: TimePointSec { seconds: 100 },
            cert_hash: Checksum256::default(),
            active: true,
            precision_score: DEFAULT_INITIAL_PRECISION_SCORE,
            accurate_deliveries: 0,
            regressions_recorded: 0,
        };

        let after_pass = apply_precision_score_update(verifier, badge, 9200, true);
        assert_eq!(after_pass.precision_score, 9200);
        assert_eq!(after_pass.accurate_deliveries, 1);
        assert_eq!(after_pass.regressions_recorded, 0);

        let after_regression = apply_precision_score_update(verifier, after_pass, 7600, false);
        assert_eq!(after_regression.precision_score, 7600);
        assert_eq!(after_regression.accurate_deliveries, 1);
        assert_eq!(after_regression.regressions_recorded, 1);
    }

    #[test]
    #[should_panic(expected = "Cannot report on your own precision score")]
    fn test_self_reporting_precision_score_is_rejected() {
        let alice = account!("alice");
        let badge = DomainBadgeRecord {
            account: alice,
            domain: "rust_consensus".to_string(),
            issuer: account!("guild"),
            issued_at: TimePointSec { seconds: 100 },
            cert_hash: Checksum256::default(),
            active: true,
            precision_score: DEFAULT_INITIAL_PRECISION_SCORE,
            accurate_deliveries: 0,
            regressions_recorded: 0,
        };
        let _ = apply_precision_score_update(alice, badge, 9900, true);
    }
}
