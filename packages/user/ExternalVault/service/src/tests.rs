#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    #[psibase::test_case(packages("ExternalVault"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::domain::{
        build_commercial_split_payload, build_corridor_receipt, build_remittance_multisig_payload,
        compute_corridor_split, compute_split_amounts, validate_rail_type,
        validate_settlement_caller, validate_zakat_remittance_split,
    };
    use crate::tables::tables::{CorridorSplitMode, ZakatRemittanceIntent};
    use psibase::fracpack::{Pack, Unpack};
    use psibase::{account, Checksum256};

    #[test]
    fn given_commercial_gross_amounts_when_split_computed_then_conserves_exact_total_without_overflow() {
        let (lender, shield, saas) = compute_split_amounts(10_000);
        assert_eq!((lender, shield, saas), (9_300, 400, 300));
        assert_eq!(lender + shield + saas, 10_000);

        // Unaligned amount assigns rounding remainder to owner (consistent with LeaseEngine)
        let (l, sh, s) = compute_split_amounts(1_001);
        assert_eq!((l, sh, s), (931, 40, 30));
        assert_eq!(l + sh + s, 1_001);

        // Huge amount near u64::MAX must not overflow u64 multiplication
        let (hl, hsh, hs) = compute_split_amounts(u64::MAX);
        assert_eq!((hl as u128) + (hsh as u128) + (hs as u128), u64::MAX as u128);

        // EVM Safe payload uses canonical mut-shield and psi-saas slugs
        let payload = build_commercial_split_payload(
            1,
            "evm_safe",
            10_000,
            account!("owner"),
            "acct_1",
            lender,
            shield,
            saas,
        );
        assert!(payload.contains("\"to\":\"mut-shield\""));
        assert!(payload.contains("\"to\":\"psi-saas\""));
    }

    #[test]
    fn given_zakat_tamlik_100_when_validated_then_routes_100_percent_principal_and_separate_sadaqah_tip() {
        let commitment = Checksum256::from([0x7a; 32]);
        let (recipient, tip, fee) = validate_zakat_remittance_split(
            CorridorSplitMode::ZAKAT_TAMLIK_100,
            "TR",
            "0xSafeIstanbulBridgeVault000000000000000001",
            100_000,
            2_500,
            0,
            &commitment,
        );

        assert_eq!((recipient, tip, fee), (100_000, 2_500, 0));

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

        // Zero tip omits ops leg
        let no_tip_payload = build_remittance_multisig_payload(
            CorridorSplitMode::ZAKAT_TAMLIK_100,
            "evm_safe",
            "TR",
            "0xSafeIstanbulBridgeVault000000000000000001",
            None,
            recipient,
            0,
            0,
            43,
            &commitment,
        );
        assert!(!no_tip_payload.contains("sadaqah_infra_tip"));
    }

    #[test]
    fn given_commercial_corridor_mode_when_split_computed_then_routes_93_to_recipient_and_7_to_protocol() {
        let (recipient, tip, fee) =
            compute_corridor_split(CorridorSplitMode::COMMERCIAL_93_4_3, 10_000, 500, 0);
        assert_eq!((recipient, tip, fee), (9_300, 500, 700));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Tamlik Violation")]
    fn given_zakat_tamlik_100_when_nonzero_fee_deducted_then_panics() {
        compute_corridor_split(CorridorSplitMode::ZAKAT_TAMLIK_100, 50_000, 500, 1);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "PII Guard Violation")]
    fn given_zero_commitment_hash_when_validating_remittance_then_panics() {
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
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "rail_type must be non-empty")]
    fn given_blank_rail_type_when_validated_then_rejected() {
        validate_rail_type("   ");
    }

    #[test]
    fn given_corridor_split_mode_when_fracpacked_then_roundtrips_with_commercial_default() {
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
        validate_settlement_caller(
            account!("intruder"),
            account!("donor"),
            account!("ext-vault"),
            false,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "already been settled")]
    fn given_settled_zakat_intent_when_reconfirmed_then_rejected() {
        validate_settlement_caller(
            account!("donor"),
            account!("donor"),
            account!("ext-vault"),
            true,
        );
    }
}
