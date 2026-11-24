#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    #[psibase::test_case(packages("WaqfTreasury"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::domain::{
        apply_disbursement_approval, apply_key_recovery_approval, validate_disbursement_rules,
        validate_endowment_params, validate_key_recovery_proposal,
    };
    use crate::tables::tables::{DisbursementProposal, Endowment, KeyRecoveryProposal};
    use psibase::{account, Checksum256};

    fn sample_endowment() -> Endowment {
        let board = vec![account!("chair"), account!("treas"), account!("trustee")];
        let shurut = Checksum256::from([0x11; 32]);
        let scholar = Checksum256::from([0x22; 32]);
        let cats = vec!["scholarship".into(), "masjid_utilities".into()];
        validate_endowment_params("Al-Noor Waqf", 1_000_000, &shurut, &scholar, &cats, &board, 2);
        Endowment {
            endowment_id: 1,
            founder: account!("masjid"),
            label: "Al-Noor Waqf".into(),
            corpus_principal: 1_000_000,
            yield_balance: 50_000,
            total_disbursed: 0,
            shurut_hash: shurut,
            scholar_attestation_hash: scholar,
            allowed_categories: cats,
            max_single_disbursement: 30_000,
            board_members: board,
            quorum_threshold: 2,
            principal_locked: true,
        }
    }

    #[test]
    fn given_shurut_locked_endowment_when_two_of_three_board_approve_yield_disbursement_then_corpus_is_preserved_and_receipt_emitted() {
        let mut e = sample_endowment();
        let mut p = DisbursementProposal {
            proposal_id: 1,
            endowment_id: 1,
            proposer: account!("chair"),
            recipient: account!("school"),
            category: "scholarship".into(),
            amount: 20_000,
            evidence_commitment_hash: Checksum256::from([0x33; 32]),
            approvals: vec![account!("chair")],
            executed: false,
            receipt_leaf_hash: None,
        };

        let receipt = apply_disbursement_approval(&mut e, &mut p, account!("treas"));
        assert!(receipt.is_some());
        assert!(p.executed);
        assert_eq!(e.corpus_principal, 1_000_000);
        assert_eq!(e.yield_balance, 30_000);
        assert_eq!(e.total_disbursed, 20_000);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Waqf Corpus Lock Violation")]
    fn given_shurut_locked_endowment_when_disbursement_exceeds_yield_then_corpus_lock_rejects() {
        let e = sample_endowment();
        validate_disbursement_rules(
            &e,
            account!("school"),
            "scholarship",
            60_000,
            &Checksum256::from([0x33; 32]),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Shurut policy check failed")]
    fn given_shurut_locked_endowment_when_category_is_disallowed_then_rejected() {
        let e = sample_endowment();
        validate_disbursement_rules(
            &e,
            account!("vendor"),
            "unapproved_luxury",
            10_000,
            &Checksum256::from([0x33; 32]),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "allowed_categories")]
    fn given_empty_shurut_category_when_creating_endowment_then_rejected() {
        let board = vec![account!("chair"), account!("treas")];
        validate_endowment_params(
            "Al-Noor Waqf",
            1_000_000,
            &Checksum256::from([0x11; 32]),
            &Checksum256::from([0x22; 32]),
            &["  ".into()],
            &board,
            2,
        );
    }

    #[test]
    fn given_compromised_treasurer_when_board_quorum_approves_key_recovery_then_seat_rotates_and_stale_approvals_expire() {
        let mut e = sample_endowment();
        let spki = Checksum256::from([0x44; 32]);
        validate_key_recovery_proposal(&e, account!("chair"), account!("treas"), account!("treas2"), &spki);

        // Pending disbursement approved by the soon-to-be-rotated treasurer
        let mut pending = DisbursementProposal {
            proposal_id: 2,
            endowment_id: 1,
            proposer: account!("treas"),
            recipient: account!("school"),
            category: "scholarship".into(),
            amount: 10_000,
            evidence_commitment_hash: Checksum256::from([0x55; 32]),
            approvals: vec![account!("treas")],
            executed: false,
            receipt_leaf_hash: None,
        };

        let mut rec = KeyRecoveryProposal {
            recovery_id: 1,
            endowment_id: 1,
            old_member: account!("treas"),
            new_member: account!("treas2"),
            new_pubkey_spki_hash: spki,
            approvals: vec![account!("chair")],
            executed: false,
        };
        let done = apply_key_recovery_approval(&mut e, &mut rec, account!("trustee"));
        assert!(done);
        assert!(rec.executed);
        assert!(e.board_members.contains(&account!("treas2")));
        assert!(!e.board_members.contains(&account!("treas")));

        // Chair's single active approval must NOT settle because rotated-out `treas` no longer counts
        assert!(apply_disbursement_approval(&mut e, &mut pending, account!("chair")).is_none());
        assert!(!pending.executed);
        // Second active board member settles it
        assert!(apply_disbursement_approval(&mut e, &mut pending, account!("treas2")).is_some());
        assert!(pending.executed);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "new_member is already on the endowment board")]
    fn given_concurrent_key_recovery_for_same_new_member_when_second_executes_then_duplicate_seat_is_rejected() {
        let mut e = sample_endowment();
        let mut rec1 = KeyRecoveryProposal {
            recovery_id: 1,
            endowment_id: 1,
            old_member: account!("treas"),
            new_member: account!("treas2"),
            new_pubkey_spki_hash: Checksum256::from([0x44; 32]),
            approvals: vec![account!("chair")],
            executed: false,
        };
        let mut rec2 = KeyRecoveryProposal {
            recovery_id: 2,
            endowment_id: 1,
            old_member: account!("chair"),
            new_member: account!("treas2"),
            new_pubkey_spki_hash: Checksum256::from([0x66; 32]),
            approvals: vec![account!("trustee")],
            executed: false,
        };
        assert!(apply_key_recovery_approval(&mut e, &mut rec1, account!("trustee")));
        // Now `treas2` is already seated, so approving rec2 must reject rather than create duplicate seats
        apply_key_recovery_approval(&mut e, &mut rec2, account!("treas2"));
    }
}
