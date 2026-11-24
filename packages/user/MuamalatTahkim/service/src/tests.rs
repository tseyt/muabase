#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    #[psibase::test_case(packages("MuamalatTahkim"))]
    fn package_installs(_chain: psibase::Chain) -> Result<(), psibase::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        apply_milestone_release, apply_raise_dispute, apply_tahkim_vote, build_validated_contract,
        parse_ruling, CONTRACT_KIND_ISTISNA, MAX_MILESTONES_PER_CONTRACT, RULING_AWARD_CONTRACTOR,
        RULING_REFUND_FUNDER, RULING_SPLIT, STATUS_DISPUTED, STATUS_FUNDED, STATUS_SETTLED,
    };
    use crate::tables::tables::ContractTemplate;
    use psibase::{account, Checksum256};

    fn sample_template() -> ContractTemplate {
        ContractTemplate {
            template_id: 1,
            contract_kind: CONTRACT_KIND_ISTISNA.into(),
            scholar_account: account!("scholar"),
            template_hash: Checksum256::from([0xaa; 32]),
            certificate_hash: Checksum256::from([0xbb; 32]),
            standard_ref: "AAOIFI-11".into(),
            active: true,
        }
    }

    #[test]
    fn given_scholar_attested_istisna_when_milestones_released_then_escrow_settles_with_receipt() {
        let tpl = sample_template();
        let mut c = build_validated_contract(
            1,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![40_000, 60_000],
            Checksum256::from([0xcc; 32]),
        );
        c.status = STATUS_FUNDED.into();

        let r1 = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x01; 32]));
        assert_ne!(r1, Checksum256::from([0u8; 32]));
        assert_eq!(c.released_amount, 40_000);
        assert_eq!(c.status, STATUS_FUNDED);

        let r2 = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x02; 32]));
        assert_ne!(r2, r1);
        assert_eq!(c.released_amount, 100_000);
        assert_eq!(c.status, STATUS_SETTLED);
    }

    #[test]
    fn given_disputed_contract_when_two_of_three_tahkim_vote_split_then_remaining_escrow_conserves_and_settles() {
        let tpl = sample_template();
        let mut c = build_validated_contract(
            2,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![40_000, 60_000],
            Checksum256::from([0xcc; 32]),
        );
        c.status = STATUS_FUNDED.into();
        let _ = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x01; 32]));

        let mut d = apply_raise_dispute(&mut c, account!("builder"), Checksum256::from([0xdd; 32]));
        assert_eq!(c.status, STATUS_DISPUTED);

        let ruling = parse_ruling(RULING_SPLIT, 20_000, 40_000);
        assert!(apply_tahkim_vote(&mut c, &mut d, account!("arb1"), ruling.clone()).is_none());
        let award = apply_tahkim_vote(&mut c, &mut d, account!("arb2"), ruling);
        assert!(award.is_some());
        assert!(d.resolved);
        assert_eq!(c.status, STATUS_SETTLED);
        assert_eq!(c.released_amount, 80_000);
        assert_eq!(c.refunded_amount, 20_000);
        assert_eq!(c.released_amount + c.refunded_amount, c.total_escrow);
    }

    #[test]
    fn given_disputed_contract_when_panel_awards_contractor_or_refunds_funder_then_settles_exact_remaining() {
        let tpl = sample_template();
        for (cid, kind, exp_rel, exp_ref) in [
            (10, RULING_AWARD_CONTRACTOR, 50_000, 0),
            (11, RULING_REFUND_FUNDER, 0, 50_000),
        ] {
            let mut c = build_validated_contract(
                cid,
                &tpl,
                &tpl.template_hash,
                account!("buyer"),
                account!("builder"),
                vec![account!("arb1"), account!("arb2"), account!("arb3")],
                vec![50_000],
                Checksum256::from([0xcc; 32]),
            );
            c.status = STATUS_FUNDED.into();
            let mut d = apply_raise_dispute(&mut c, account!("buyer"), Checksum256::from([0xdd; 32]));
            let ruling = parse_ruling(kind, 0, 0);
            assert!(apply_tahkim_vote(&mut c, &mut d, account!("arb1"), ruling.clone()).is_none());
            assert!(apply_tahkim_vote(&mut c, &mut d, account!("arb3"), ruling).is_some());
            assert_eq!(c.released_amount, exp_rel);
            assert_eq!(c.refunded_amount, exp_ref);
            assert_eq!(c.status, STATUS_SETTLED);
        }
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Milestone release blocked")]
    fn given_disputed_contract_when_funder_attempts_unilateral_release_then_blocked() {
        let tpl = sample_template();
        let mut c = build_validated_contract(
            3,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![50_000],
            Checksum256::from([0xcc; 32]),
        );
        c.status = STATUS_FUNDED.into();
        let _ = apply_raise_dispute(&mut c, account!("buyer"), Checksum256::from([0xdd; 32]));
        let _ = apply_milestone_release(&mut c, account!("buyer"), &Checksum256::from([0x01; 32]));
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Scholar template attestation verification failed")]
    fn given_revoked_template_when_creating_contract_then_rejected() {
        let mut tpl = sample_template();
        tpl.active = false;
        let _ = build_validated_contract(
            4,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![50_000],
            Checksum256::from([0xcc; 32]),
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "milestone_amounts cannot exceed")]
    fn given_excessive_milestone_count_when_creating_contract_then_rejected_to_prevent_entry_id_collision() {
        let tpl = sample_template();
        let _ = build_validated_contract(
            5,
            &tpl,
            &tpl.template_hash,
            account!("buyer"),
            account!("builder"),
            vec![account!("arb1"), account!("arb2"), account!("arb3")],
            vec![1; MAX_MILESTONES_PER_CONTRACT + 1],
            Checksum256::from([0xcc; 32]),
        );
    }
}
