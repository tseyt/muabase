#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    #[psibase::test_case(packages("LeaseEngine"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::domain::{
        validate_late_return, validate_lease_creation, validate_return_finalization,
        validate_settlement_and_compute_split, DEFAULT_MUTUAL_SHIELD_FEE_BPS,
        DEFAULT_PROTOCOL_SAAS_FEE_BPS,
    };
    use crate::tables::tables::{UsufructLease, STATUS_ACTIVE, STATUS_COMPLETED, STATUS_CREATED};
    use psibase::{account, AccountNumber, Checksum256};

    fn sample_lease(status: u8) -> UsufructLease {
        UsufructLease {
            lease_id: 1,
            lessor: account!("lessor-1"),
            lessee: account!("lessee-1"),
            token_id: 10,
            nft_id: None,
            duration_sec: 86_400,
            gross_payment: 100_0000.into(),
            payment_token_id: 1,
            start_time: None,
            due_time: None,
            returned_time: None,
            pre_condition_evidence_hash: Checksum256::from([0x11; 32]),
            post_condition_evidence_hash: None,
            status,
            is_progressive_buyout: false,
            buyout_equity_units: 0.into(),
            protocol_saas_fee_bps: DEFAULT_PROTOCOL_SAAS_FEE_BPS,
            mutual_shield_fee_bps: DEFAULT_MUTUAL_SHIELD_FEE_BPS,
            custodian_fee_bps: 200,
            custodian_account: account!("custodian"),
            charity_sink_account: account!("waqf-sink"),
        }
    }

    #[test]
    fn given_valid_lease_params_when_created_and_settled_then_commercial_split_conserves_gross() {
        let pre = Checksum256::from([0xaa; 32]);
        let split = validate_lease_creation(
            account!("lessee-1"),
            account!("lessor-1"),
            86_400,
            100_0000,
            &pre,
            account!("custodian"),
            200,
            account!("waqf-sink"),
        );
        assert_eq!(split.saas_amount, 3_0000);
        assert_eq!(split.shield_amount, 4_0000);
        assert_eq!(split.custodian_amount, 2_0000);
        assert_eq!(split.net_owner_amount, 91_0000);
        assert_eq!(
            split.saas_amount + split.shield_amount + split.custodian_amount + split.net_owner_amount,
            100_0000
        );

        let lease = sample_lease(STATUS_CREATED);
        let settled =
            validate_settlement_and_compute_split(&lease, account!("lease-eng"), account!("lease-eng"));
        assert_eq!(settled, split);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "custodian_account must be set")]
    fn given_nonzero_custodian_fee_and_zero_account_when_creating_lease_then_rejected() {
        let _ = validate_lease_creation(
            account!("lessee-1"),
            account!("lessor-1"),
            86_400,
            100_0000,
            &Checksum256::from([0xaa; 32]),
            AccountNumber::new(0),
            100,
            account!("waqf-sink"),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "already settled")]
    fn given_already_active_lease_when_replaying_settlement_then_rejected() {
        let lease = sample_lease(STATUS_ACTIVE);
        let _ = validate_settlement_and_compute_split(
            &lease,
            account!("lessee-1"),
            account!("lease-eng"),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Riba Violation")]
    fn given_lessor_as_charity_sink_when_processing_late_return_then_riba_violation_panics() {
        let mut lease = sample_lease(STATUS_ACTIVE);
        lease.charity_sink_account = lease.lessor;
        validate_late_return(
            &lease,
            lease.lessor,
            account!("lease-eng"),
            &Checksum256::from([0xbb; 32]),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Lease not active")]
    fn given_completed_lease_when_replaying_late_return_then_rejected() {
        let lease = sample_lease(STATUS_COMPLETED);
        validate_return_finalization(
            &lease,
            lease.lessor,
            account!("lease-eng"),
            &Checksum256::from([0xbb; 32]),
        );
    }
}
