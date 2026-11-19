#![allow(non_snake_case)]

#[cfg(all(test, target_family = "wasm"))]
mod tests {
    use crate::Wrapper;
    use psibase::fracpack::Pack;
    use psibase::services::accounts::Wrapper as Accounts;
    use psibase::services::auth_delegate::Wrapper as AuthDelegate;
    use psibase::services::nft as Nft;
    use psibase::*;
    use serde::Deserialize;
    use serde_json::Value;
    use Nft::Nft as NftRecord;

    #[derive(Deserialize)]
    struct Response {
        data: Data,
    }

    #[derive(Deserialize)]
    struct Data {
        nfts: UserNfts,
    }

    #[derive(Deserialize)]
    struct UserNfts {
        edges: Vec<Edge>,
    }

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct Edge {
        node: NftRecord,
    }

    fn get_nr_nfts(chain: &Chain, user: AccountNumber) -> Result<usize, psibase::Error> {
        let res: Value = chain.graphql(
            Nft::SERVICE,
            &format!(
                r#"
                query Nfts {{
                    nfts(user: "{}") {{
                        edges {{
                            node {{
                                id
                                issuer
                                owner
                            }}
                        }}
                    }}
                }}"#,
                user
            ),
        )?;

        let response_root: Response = serde_json::from_value(res)?;
        Ok(response_root.data.nfts.edges.len())
    }

    #[psibase::test_case(packages("StagedTx", "Nft", "AuthSig"))]
    fn test_propose(chain: psibase::Chain) -> Result<(), psibase::Error> {
        Wrapper::push(&chain).init();

        let alice = account!("alice");
        chain.new_account(alice).unwrap();

        let bob = account!("bob");
        chain.new_account(bob).unwrap();

        assert_eq!(get_nr_nfts(&chain, bob)?, 0);

        let id = Wrapper::push_from(&chain, alice)
            .propose(
                vec![Action {
                    sender: bob,
                    service: Nft::SERVICE,
                    method: MethodNumber::from("mint"),
                    rawData: Nft::action_structs::mint { nftId: None }.packed().into(),
                }],
                true,
            )
            .get()?;

        assert_eq!(get_nr_nfts(&chain, bob)?, 0);

        let txid = Wrapper::push(&chain).get_staged_tx(id).get()?.txid;

        Wrapper::push_from(&chain, bob).accept(id, txid).get()?;

        assert_eq!(get_nr_nfts(&chain, bob)?, 1);

        Ok(())
    }

    struct TestTransaction<'a> {
        chain: &'a Chain,
        id: u32,
        account: AccountNumber,
    }

    impl<'a> TestTransaction<'a> {
        // Creates a new staged transaction that must be approved
        // by all the given accounts
        pub fn new(chain: &'a Chain, accounts: &[AccountNumber]) -> Result<Self, Error> {
            let id = Wrapper::push(&chain)
                .propose(
                    accounts
                        .iter()
                        .map(|account| Nft::Wrapper::pack_from(*account).mint(None))
                        .collect(),
                    true,
                )
                .get()?;
            Ok(Self {
                chain,
                id,
                account: accounts[0],
            })
        }
        // Returns true if the staged transaction was executed
        pub fn approve(&self, account: AccountNumber) -> Result<bool, Error> {
            let prev_nfts = get_nr_nfts(self.chain, self.account)?;
            let txid = Wrapper::push(self.chain).get_staged_tx(self.id).get()?.txid;
            Wrapper::push_from(self.chain, account)
                .accept(self.id, txid)
                .get()?;
            Ok(get_nr_nfts(self.chain, self.account)? != prev_nfts)
        }
    }

    #[psibase::test_case(packages("StagedTx"))]
    fn test_change_auth_serv(chain: psibase::Chain) -> Result<(), psibase::Error> {
        let alice = account!("alice");
        chain.new_account(alice)?;
        let bob = account!("bob");
        chain.new_account(bob)?;
        let carol = account!("carol");
        chain.new_account(carol)?;

        let tx = TestTransaction::new(&chain, &[alice, bob])?;
        assert!(!tx.approve(alice)?);

        // Setting the account owner should invalidate the approval from alice
        AuthDelegate::push_from(&chain, alice)
            .setOwner(carol)
            .get()?;
        Accounts::push_from(&chain, alice)
            .setAuthServ(AuthDelegate::SERVICE)
            .get()?;

        assert!(!tx.approve(bob)?);
        assert!(tx.approve(carol)?);
        Ok(())
    }

    #[psibase::test_case(packages("StagedTx"))]
    fn test_change_set_unused_owner(chain: psibase::Chain) -> Result<(), psibase::Error> {
        let alice = account!("alice");
        chain.new_account(alice)?;
        let bob = account!("bob");
        chain.new_account(bob)?;
        let carol = account!("carol");
        chain.new_account(carol)?;

        let tx = TestTransaction::new(&chain, &[alice, bob])?;
        assert!(!tx.approve(alice)?);

        // This should have no effect, because auth-delegate is not alice's
        // auth service
        AuthDelegate::push_from(&chain, alice)
            .setOwner(carol)
            .get()?;

        assert!(tx.approve(bob)?);

        Ok(())
    }

    #[psibase::test_case(packages("StagedTx"))]
    fn test_change_owner(chain: psibase::Chain) -> Result<(), psibase::Error> {
        let alice = account!("alice");
        chain.new_account(alice)?;
        let bob = account!("bob");
        chain.new_account(bob)?;
        let carol = account!("carol");
        chain.new_account(carol)?;
        let dave = account!("dave");
        chain.new_account(dave)?;

        AuthDelegate::push_from(&chain, alice)
            .setOwner(carol)
            .get()?;
        Accounts::push_from(&chain, alice)
            .setAuthServ(AuthDelegate::SERVICE)
            .get()?;

        let tx = TestTransaction::new(&chain, &[alice, bob])?;
        assert!(!tx.approve(alice)?);

        // Changing the account owner should invalidate the approval from alice
        AuthDelegate::push_from(&chain, alice)
            .setOwner(dave)
            .get()?;

        assert!(!tx.approve(bob)?);
        assert!(tx.approve(dave)?);

        Ok(())
    }

    #[psibase::test_case(packages("StagedTx"))]
    fn test_change_auth_chained(chain: psibase::Chain) -> Result<(), psibase::Error> {
        let alice = account!("alice");
        chain.new_account(alice)?;
        let bob = account!("bob");
        chain.new_account(bob)?;
        let carol = account!("carol");
        chain.new_account(carol)?;
        let dave = account!("dave");
        chain.new_account(dave)?;

        AuthDelegate::push_from(&chain, alice)
            .setOwner(carol)
            .get()?;
        Accounts::push_from(&chain, alice)
            .setAuthServ(AuthDelegate::SERVICE)
            .get()?;

        let tx = TestTransaction::new(&chain, &[alice, bob])?;
        assert!(!tx.approve(carol)?);

        // Changing the account owner should invalidate the approval from carol
        AuthDelegate::push_from(&chain, carol)
            .setOwner(dave)
            .get()?;
        Accounts::push_from(&chain, carol)
            .setAuthServ(AuthDelegate::SERVICE)
            .get()?;

        assert!(!tx.approve(bob)?);
        assert!(tx.approve(dave)?);

        Ok(())
    }

    #[psibase::test_case(packages("StagedTx"))]
    fn test_change_owner_chained(chain: psibase::Chain) -> Result<(), psibase::Error> {
        let alice = account!("alice");
        chain.new_account(alice)?;
        let bob = account!("bob");
        chain.new_account(bob)?;
        let carol = account!("carol");
        chain.new_account(carol)?;
        let dave = account!("dave");
        chain.new_account(dave)?;
        let eliza = account!("eliza");
        chain.new_account(eliza)?;

        AuthDelegate::push_from(&chain, alice)
            .setOwner(carol)
            .get()?;
        Accounts::push_from(&chain, alice)
            .setAuthServ(AuthDelegate::SERVICE)
            .get()?;
        AuthDelegate::push_from(&chain, carol)
            .setOwner(dave)
            .get()?;
        Accounts::push_from(&chain, carol)
            .setAuthServ(AuthDelegate::SERVICE)
            .get()?;

        let tx = TestTransaction::new(&chain, &[alice, bob])?;
        assert!(!tx.approve(carol)?);

        // Changing the account owner should invalidate the approval from carol
        AuthDelegate::push_from(&chain, carol)
            .setOwner(eliza)
            .get()?;

        assert!(!tx.approve(bob)?);
        assert!(tx.approve(eliza)?);

        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::db::impls::build_executed_attestation;
    use crate::db::tables::{ActionList, StagedTx, WitnessAttestation};
    use psibase::{account, Checksum256, TimePointUSec};

    #[test]
    fn test_witness_eligibility_requires_valid_score_and_badge() {
        let proposer = account!("alice");
        let lessee = account!("bob");
        let witness = account!("nour");

        crate::service::validate_witness_eligibility(
            witness,
            proposer,
            &[proposer, lessee],
            Some("master_appraiser"),
            "master_appraiser",
            true,
            9500,
            9100,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Witness trust_score 7999 is below the required 8000")]
    fn test_witness_rejected_when_trust_score_below_threshold() {
        let proposer = account!("alice");
        let witness = account!("nour");

        crate::service::validate_witness_eligibility(
            witness,
            proposer,
            &[proposer],
            None,
            "master_appraiser",
            true,
            7999,
            9100,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Witness precision_score 7999 is below the required 8000")]
    fn test_witness_rejected_when_precision_score_below_threshold() {
        let proposer = account!("alice");
        let witness = account!("nour");

        crate::service::validate_witness_eligibility(
            witness,
            proposer,
            &[proposer],
            Some("master_appraiser"),
            "master_appraiser",
            true,
            9500,
            7999,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Witness does not hold an active domain badge: master_appraiser")]
    fn test_witness_rejected_without_active_domain_badge() {
        let proposer = account!("alice");
        let witness = account!("nour");

        crate::service::validate_witness_eligibility(
            witness,
            proposer,
            &[proposer],
            None,
            "master_appraiser",
            false,
            9500,
            9100,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Witness must attest with the required domain badge: master_appraiser")]
    fn test_witness_rejected_when_domain_badge_mismatches_required_domain() {
        let proposer = account!("alice");
        let witness = account!("nour");

        crate::service::validate_witness_eligibility(
            witness,
            proposer,
            &[proposer],
            Some("master_appraiser"),
            "sommelier_l3",
            true,
            9500,
            9100,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Proposer cannot witness their own staged tx")]
    fn test_proposer_cannot_witness_own_staged_tx() {
        let proposer = account!("alice");

        crate::service::validate_witness_eligibility(
            proposer,
            proposer,
            &[],
            None,
            "master_appraiser",
            true,
            9900,
            9500,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "A party to the staged tx cannot act as its witness")]
    fn test_party_to_staged_tx_cannot_act_as_witness() {
        let proposer = account!("alice");
        let party = account!("bob");

        crate::service::validate_witness_eligibility(
            party,
            proposer,
            &[party],
            None,
            "master_appraiser",
            true,
            9900,
            9500,
        );
    }

    #[test]
    fn test_executed_attestation_archives_provenance_and_parent_txid_on_execution() {
        let proposer = account!("alice");
        let witness = account!("nour");
        let parent_txid = Checksum256::from([0x11; 32]);
        let txid = Checksum256::from([0x22; 32]);
        let ricardian_hash = Checksum256::from([0x33; 32]);

        let staged_tx = StagedTx {
            id: 42,
            txid: txid.clone(),
            propose_block: 10,
            propose_date: TimePointUSec { microseconds: 1_000_000 },
            proposer,
            action_list: ActionList { actions: vec![] },
            auto_exec: true,
            ricardian_hash: Some(ricardian_hash.clone()),
            required_witnesses: 2,
            required_domain: Some("rust_consensus".to_string()),
            parent_txid: Some(parent_txid.clone()),
        };

        let attestation = WitnessAttestation {
            id: 42,
            witness,
            trust_score: 9400,
            precision_score: 9100,
            domain_badge: "rust_consensus".to_string(),
            timestamp: TimePointUSec { microseconds: 2_000_000 },
        };

        let archived = build_executed_attestation(
            &staged_tx,
            &attestation,
            TimePointUSec { microseconds: 3_000_000 },
        );

        assert_eq!(archived.id, 42);
        assert_eq!(archived.txid, txid);
        assert_eq!(archived.parent_txid, Some(parent_txid));
        assert_eq!(archived.ricardian_hash, Some(ricardian_hash));
        assert_eq!(archived.witness, witness);
        assert_eq!(archived.trust_score, 9400);
        assert_eq!(archived.precision_score, 9100);
        assert_eq!(archived.domain_badge, "rust_consensus");
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "Bayyinah Violation: PersonalRemittanceAttestation requires witness_quorum >= 2, got 1"
    )]
    fn test_remittance_attestation_rejects_witness_quorum_below_two() {
        let proposer = account!("donor");
        let commitment = Checksum256::from([0xab; 32]);
        let declaration = Checksum256::from([0xcd; 32]);

        crate::service::build_remittance_attestation(
            1,
            proposer,
            commitment,
            "TR".to_string(),
            declaration,
            Some(crate::db::tables::AsnafCategory::FUQARA),
            1,
            TimePointUSec { microseconds: 1_000_000 },
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "Remittance escrow release blocked: witness quorum not met (1 of 2 verified humanitarian-verifier witnesses)"
    )]
    fn test_remittance_escrow_release_blocked_when_quorum_has_fewer_than_two_witnesses() {
        let proposer = account!("donor");
        let witness_one = account!("verifier1");
        let commitment = Checksum256::from([0x44; 32]);
        let declaration = Checksum256::from([0x55; 32]);

        let att = crate::service::build_remittance_attestation(
            7,
            proposer,
            commitment.clone(),
            "TR".to_string(),
            declaration,
            Some(crate::db::tables::AsnafCategory::MASAKIN),
            2,
            TimePointUSec { microseconds: 1_000_000 },
        );

        let partial = crate::service::apply_remittance_witness_attestation(
            att,
            witness_one,
            &commitment,
            "humanitarian-verifier",
            true,
            9200,
            8800,
        );
        assert!(!partial.quorum_satisfied);
        assert_eq!(partial.verified_witnesses.len(), 1);

        crate::service::assert_remittance_quorum_satisfied(&partial, &commitment);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "Witness does not hold an active domain badge: humanitarian-verifier"
    )]
    fn test_remittance_attestation_blocks_witness_missing_humanitarian_badge() {
        let proposer = account!("donor");
        let witness = account!("unbadged");
        let commitment = Checksum256::from([0x66; 32]);
        let declaration = Checksum256::from([0x77; 32]);

        let att = crate::service::build_remittance_attestation(
            8,
            proposer,
            commitment.clone(),
            "UA".to_string(),
            declaration,
            Some(crate::db::tables::AsnafCategory::IBN_AL_SABIL),
            2,
            TimePointUSec { microseconds: 1_000_000 },
        );

        crate::service::apply_remittance_witness_attestation(
            att,
            witness,
            &commitment,
            "humanitarian-verifier",
            false,
            9500,
            9100,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "Witness must attest with the required domain badge: humanitarian-verifier"
    )]
    fn test_remittance_attestation_rejects_wrong_domain_badge() {
        let proposer = account!("donor");
        let witness = account!("appraiser");
        let commitment = Checksum256::from([0x66; 32]);
        let declaration = Checksum256::from([0x77; 32]);

        let att = crate::service::build_remittance_attestation(
            9,
            proposer,
            commitment.clone(),
            "AE".to_string(),
            declaration,
            Some(crate::db::tables::AsnafCategory::GHARIMIN),
            2,
            TimePointUSec { microseconds: 1_000_000 },
        );

        crate::service::apply_remittance_witness_attestation(
            att,
            witness,
            &commitment,
            "master_appraiser",
            true,
            9500,
            9100,
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(expected = "Witness trust_score 7999 is below the required 8000")]
    fn test_remittance_attestation_rejects_witness_below_8000_trust_score() {
        let proposer = account!("donor");
        let witness = account!("verifier1");
        let commitment = Checksum256::from([0x88; 32]);
        let declaration = Checksum256::from([0x99; 32]);

        let att = crate::service::build_remittance_attestation(
            10,
            proposer,
            commitment.clone(),
            "UZ".to_string(),
            declaration,
            Some(crate::db::tables::AsnafCategory::FUQARA),
            2,
            TimePointUSec { microseconds: 1_000_000 },
        );

        crate::service::apply_remittance_witness_attestation(
            att,
            witness,
            &commitment,
            "humanitarian-verifier",
            true,
            7999,
            9000,
        );
    }

    #[test]
    fn test_remittance_attestation_unlocks_quorum_with_two_verified_humanitarian_witnesses() {
        use psibase::fracpack::{Pack, Unpack};

        let proposer = account!("donor");
        let witness_one = account!("qirimv1");
        let witness_two = account!("qirimv2");
        let digest_bytes = crate::service::validate_32_byte_commitment_hex(
            "0x9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        );
        let commitment = Checksum256::from(digest_bytes);
        let declaration = Checksum256::from([0xaa; 32]);

        let att = crate::service::build_remittance_attestation(
            11,
            proposer,
            commitment.clone(),
            "TR".to_string(),
            declaration,
            Some(crate::db::tables::AsnafCategory::FUQARA),
            2,
            TimePointUSec { microseconds: 1_500_000 },
        );

        let att = crate::service::apply_remittance_witness_attestation(
            att,
            witness_one,
            &commitment,
            "humanitarian-verifier",
            true,
            9100,
            8900,
        );
        assert!(!att.quorum_satisfied);

        let att = crate::service::apply_remittance_witness_attestation(
            att,
            witness_two,
            &commitment,
            "humanitarian-verifier",
            true,
            8450,
            8200,
        );
        assert!(att.quorum_satisfied);
        assert_eq!(att.verified_witnesses, vec![witness_one, witness_two]);

        crate::service::assert_remittance_quorum_satisfied(&att, &commitment);

        // Verify fracpack round-trip of AsnafCategory + PersonalRemittanceAttestation
        let packed = att.packed();
        let unpacked =
            crate::db::tables::PersonalRemittanceAttestation::unpacked(&packed).unwrap();
        assert_eq!(unpacked, att);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "PII Guard Violation: Commitment hash must be an exact 32-byte (64-hex-char) SHA-256 digest; plaintext PII is strictly forbidden"
    )]
    fn test_pii_guard_rejects_plaintext_beneficiary_name_or_iban() {
        crate::service::validate_32_byte_commitment_hex("Amina Qirimli | TR330006100519786457841326");
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "PII Guard Violation: Commitment hash must be an exact 32-byte (64-hex-char) SHA-256 digest; plaintext PII is strictly forbidden"
    )]
    fn test_pii_guard_rejects_non_32_byte_hex_digest() {
        crate::service::validate_32_byte_commitment_hex("0xdeadbeef");
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "PII Guard Violation: Commitment hash cannot be an uninitialized all-zero 32-byte digest"
    )]
    fn test_pii_guard_rejects_zero_32_byte_digest() {
        crate::service::validate_32_byte_commitment_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    #[should_panic(
        expected = "PII & Compliance Violation: corridorJurisdiction 'Istanbul, Street 12' must be a 2-letter uppercase ISO-3166-1 alpha-2 bridge code"
    )]
    fn test_pii_guard_rejects_freeform_address_in_corridor_jurisdiction() {
        let proposer = account!("donor");
        crate::service::build_remittance_attestation(
            12,
            proposer,
            Checksum256::from([0x12; 32]),
            "Istanbul, Street 12".to_string(),
            Checksum256::from([0x34; 32]),
            None,
            2,
            TimePointUSec { microseconds: 1_000_000 },
        );
    }
}
