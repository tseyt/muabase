use sha2::{Digest, Sha256};

mod db;
mod event;
mod policy;

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// A service for staged transaction proposal and execution
///
/// A staged transaction allows an account (the proposer) to propose a set of
/// actions to be executed by another account (the executor). The set of actions
/// proposed is known as the staged transaction. The staged transaction is not
/// executed unless and until the executor's auth service authorizes the execution.
///
/// The rules for authorization may vary depending on the staged-tx policy enforced
/// by the executor's auth service.
///
/// This can be used, for example, to propose a transaction on behalf of an account
/// that is only authorized by the combined authorization of multiple other accounts
/// (a.k.a. a "multi-sig" or multi-signature transaction), among many other uses.
#[psibase::service(recursive = true, tables = "db::tables")]
pub mod service {
    pub use crate::db::tables::*;
    pub use crate::event::StagedTxEvent;
    use crate::policy::StagedTxPolicy;
    use fracpack::Pack;
    use psibase::services::transact::ServiceMethod;
    use psibase::services::{
        credentials::Wrapper as Credentials, events::Wrapper as Events,
        transact::Wrapper as Transact,
    };
    use psibase::*;
    use std::collections::HashSet;

    const ENABLE_PRINT: bool = false;

    /// Minimum `trust_score` (bps) a witness needs to attest. Matches the documented
    /// adjudicator floor used by Guilds / ClaimCouncil evaluators (80.00%).
    pub const MIN_WITNESS_TRUST_SCORE: u32 = 8000;

    /// Minimum domain-scoped `precision_score` (Dabt, in bps) a witness needs to attest (80.00%).
    pub const MIN_WITNESS_PRECISION_SCORE: u32 = 8000;

    /// Canonical soulbound domain badge required to verify humanitarian / Zakat beneficiaries.
    pub const HUMANITARIAN_VERIFIER_BADGE: &str = "humanitarian-verifier";

    /// Minimum binary Bayyinah witness quorum required for PersonalRemittanceAttestation.
    pub const MIN_REMITTANCE_WITNESS_QUORUM: u8 = 2;

    /// Strictly parses and validates a 32-byte SHA-256 hex digest string (with optional `0x` prefix).
    /// Rejects any plaintext PII, malformed hex, non-32-byte length, or all-zero digest.
    pub fn validate_32_byte_commitment_hex(digest_hex: &str) -> [u8; 32] {
        let trimmed = digest_hex.strip_prefix("0x").unwrap_or(digest_hex);
        assert!(
            trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()),
            "PII Guard Violation: Commitment hash must be an exact 32-byte (64-hex-char) SHA-256 digest; plaintext PII is strictly forbidden"
        );

        let mut out = [0u8; 32];
        for (i, chunk) in trimmed.as_bytes().chunks_exact(2).enumerate() {
            let byte_str = std::str::from_utf8(chunk).unwrap();
            out[i] = u8::from_str_radix(byte_str, 16).unwrap();
        }
        assert!(
            out != [0u8; 32],
            "PII Guard Violation: Commitment hash cannot be an uninitialized all-zero 32-byte digest"
        );
        out
    }

    /// Ensures a `Checksum256` digest is non-zero (preventing empty/trivial commitment records).
    pub fn validate_non_zero_digest(digest: &Checksum256, field_name: &str) {
        assert!(
            *digest != Checksum256::from([0u8; 32]),
            "PII Guard Violation: {} must be a non-zero 32-byte salted SHA-256 commitment",
            field_name
        );
    }

    /// Validates that `corridor_jurisdiction` is a canonical uppercase ISO-3166-1 alpha-2 country code
    /// (e.g., "TR", "AE", "UA", "UZ"), preventing free-form PII leakage in table metadata.
    pub fn validate_corridor_jurisdiction(corridor: &str) {
        assert!(
            corridor.len() == 2 && corridor.chars().all(|c| c.is_ascii_uppercase()),
            "PII & Compliance Violation: corridorJurisdiction '{}' must be a 2-letter uppercase ISO-3166-1 alpha-2 bridge code (e.g. TR, AE, UA, UZ)",
            corridor
        );
    }

    pub fn is_humanitarian_verifier_badge(domain_badge: &str) -> bool {
        domain_badge == HUMANITARIAN_VERIFIER_BADGE || domain_badge == "humanitarian_verifier"
    }

    pub fn build_remittance_attestation(
        attestation_id: u64,
        proposer: AccountNumber,
        recipient_commitment_hash: Checksum256,
        corridor_jurisdiction: String,
        non_commercial_personal_declaration_hash: Checksum256,
        asnaf_category: Option<AsnafCategory>,
        witness_quorum: u8,
        created_at: TimePointUSec,
    ) -> PersonalRemittanceAttestation {
        assert!(
            witness_quorum >= MIN_REMITTANCE_WITNESS_QUORUM,
            "Bayyinah Violation: PersonalRemittanceAttestation requires witness_quorum >= {}, got {}",
            MIN_REMITTANCE_WITNESS_QUORUM,
            witness_quorum
        );
        validate_non_zero_digest(&recipient_commitment_hash, "recipientCommitmentHash");
        validate_non_zero_digest(
            &non_commercial_personal_declaration_hash,
            "nonCommercialPersonalDeclarationHash",
        );
        validate_corridor_jurisdiction(&corridor_jurisdiction);

        PersonalRemittanceAttestation {
            attestation_id,
            proposer,
            recipient_commitment_hash,
            corridor_jurisdiction,
            non_commercial_personal_declaration_hash,
            asnaf_category,
            witness_quorum,
            required_domain: HUMANITARIAN_VERIFIER_BADGE.to_string(),
            verified_witnesses: Vec::new(),
            quorum_satisfied: false,
            created_at,
        }
    }

    pub fn apply_remittance_witness_attestation(
        mut attestation: PersonalRemittanceAttestation,
        witness: AccountNumber,
        expected_commitment: &Checksum256,
        domain_badge: &str,
        has_active_badge: bool,
        trust_score: u32,
        precision_score: u32,
    ) -> PersonalRemittanceAttestation {
        validate_non_zero_digest(expected_commitment, "recipientCommitmentHash");
        assert!(
            attestation.recipient_commitment_hash == *expected_commitment,
            "Commitment hash mismatch: witness attested to a different recipientCommitmentHash"
        );
        assert!(
            is_humanitarian_verifier_badge(domain_badge),
            "Witness must attest with the required domain badge: {}",
            HUMANITARIAN_VERIFIER_BADGE
        );

        let normalized_required = if domain_badge == "humanitarian_verifier" {
            "humanitarian_verifier"
        } else {
            HUMANITARIAN_VERIFIER_BADGE
        };

        validate_witness_eligibility(
            witness,
            attestation.proposer,
            &[],
            Some(normalized_required),
            domain_badge,
            has_active_badge,
            trust_score,
            precision_score,
        );

        assert!(
            !attestation.verified_witnesses.contains(&witness),
            "Witness has already attested to this beneficiary commitment"
        );

        attestation.verified_witnesses.push(witness);
        attestation.quorum_satisfied =
            attestation.verified_witnesses.len() >= attestation.witness_quorum as usize;
        attestation
    }

    pub fn assert_remittance_quorum_satisfied(
        attestation: &PersonalRemittanceAttestation,
        expected_commitment: &Checksum256,
    ) {
        assert!(
            attestation.witness_quorum >= MIN_REMITTANCE_WITNESS_QUORUM,
            "Bayyinah Violation: witness_quorum {} is below minimum {}",
            attestation.witness_quorum,
            MIN_REMITTANCE_WITNESS_QUORUM
        );
        assert!(
            attestation.recipient_commitment_hash == *expected_commitment,
            "Recipient commitment hash mismatch against verified PersonalRemittanceAttestation"
        );
        assert!(
            attestation.verified_witnesses.len() >= attestation.witness_quorum as usize
                && attestation.quorum_satisfied,
            "Remittance escrow release blocked: witness quorum not met ({} of {} verified humanitarian-verifier witnesses)",
            attestation.verified_witnesses.len(),
            attestation.witness_quorum
        );
    }

    pub fn validate_witness_eligibility(
        witness: AccountNumber,
        proposer: AccountNumber,
        parties: &[AccountNumber],
        required_domain: Option<&str>,
        domain_badge: &str,
        has_active_badge: bool,
        trust_score: u32,
        precision_score: u32,
    ) {
        assert!(
            witness != proposer,
            "Proposer cannot witness their own staged tx"
        );
        assert!(
            !parties.contains(&witness),
            "A party to the staged tx cannot act as its witness"
        );

        if let Some(required) = required_domain {
            assert!(
                domain_badge == required,
                "Witness must attest with the required domain badge: {}",
                required
            );
        }

        assert!(
            has_active_badge,
            "Witness does not hold an active domain badge: {}",
            domain_badge
        );
        assert!(
            trust_score >= MIN_WITNESS_TRUST_SCORE,
            "Witness trust_score {} is below the required {}",
            trust_score,
            MIN_WITNESS_TRUST_SCORE
        );
        assert!(
            precision_score >= MIN_WITNESS_PRECISION_SCORE,
            "Witness precision_score {} is below the required {}",
            precision_score,
            MIN_WITNESS_PRECISION_SCORE
        );
    }

    fn debug_print(msg: &str) {
        if ENABLE_PRINT {
            psibase::write_console(msg);
        }
    }

    /// Runs before every other action except 'init', verifies that the service
    /// has been initialized.
    #[pre_action(exclude(init))]
    fn check_init() {
        let table: InitTable = InitTable::new();
        table
            .get_index_pk()
            .get(&())
            .expect("service not initialized");
    }

    /// Initialize the staged-tx service
    #[action]
    fn init() {
        let table = InitTable::new();

        if table.get_index_pk().get(&()).is_none() {
            table.put(&InitRow {}).unwrap();
            let _ = LastUsed::get_next_id(); // Create this singleton on init instead of first use.
        }

        let updated = MethodNumber::from("updated");
        Events::call().addIndex(EventDb::HistoryEvent, SERVICE, updated, 0); // Index events related to specific txid
        Events::call().addIndex(EventDb::HistoryEvent, SERVICE, updated, 1); // Index events related to specific proposer/accepter/rejecter
    }

    fn ensure_not_rejected(actions: &[Action]) {
        let mut seen = HashSet::new();
        for action in actions {
            let key = (
                action.sender.value,
                action.service.value,
                action.method.value,
            );
            if !seen.insert(key) {
                continue;
            }
            let Some(policy) = StagedTxPolicy::new(action.sender) else {
                continue;
            };
            if policy.does_reject(
                Vec::new(),
                ServiceMethod::new(action.service, action.method),
            ) {
                abort_message(&format!("sender {} rejects the action", action.sender));
            }
        }
    }

    /// Proposes a new staged transaction containing the specified actions.
    /// Returns the ID of the database record containing the staged transaction.
    ///
    /// * `actions`: The actions to be staged
    /// * `auto_exec`: Enables automatic execution as soon as the transaction has enough approvals
    #[action]
    fn propose(actions: Vec<Action>, auto_exec: bool) -> u32 {
        ensure_not_rejected(&actions);
        let new_tx = StagedTx::add(actions, auto_exec);

        emit_update(new_tx.txid.clone(), StagedTxEvent::PROPOSED);

        // A proposal is also an implicit accept
        accept(new_tx.id, new_tx.txid);

        new_tx.id
    }

    fn try_auto_execute(staged_tx: StagedTx) {
        if staged_tx.auto_exec && staged_tx.can_witness_execute() {
            let authorized = staged_tx
                .invocations()
                .iter()
                .all(|(sender, service_method)| {
                    StagedTxPolicy::new(*sender).map_or(true, |policy| {
                        policy.does_auth(staged_tx.accepters(), *service_method)
                    })
                });

            if authorized {
                execute_impl(staged_tx);
            }
        }
    }

    /// Proposes a new staged transaction with Ricardian legal hash and witness quorum
    ///
    /// * `required_witnesses`: Number of qualified witnesses that must attest before execution
    /// * `required_domain`: Domain badge every witness must hold (`None` = any active badge)
    #[action]
    #[allow(non_snake_case)]
    fn proposeRicardian(
        actions: Vec<Action>,
        auto_exec: bool,
        ricardian_hash: Option<Checksum256>,
        required_witnesses: u8,
        required_domain: Option<String>,
    ) -> u32 {
        ensure_not_rejected(&actions);
        let new_tx = StagedTx::add_with_ricardian(
            actions,
            auto_exec,
            ricardian_hash,
            required_witnesses,
            required_domain,
        );

        emit_update(new_tx.txid.clone(), StagedTxEvent::PROPOSED);

        // A proposal is also an implicit accept
        accept(new_tx.id, new_tx.txid);

        new_tx.id
    }

    /// Proposes a new staged transaction linked to a parent staged transaction txid
    /// to preserve chain continuity (Ittisal al-Sanad) across multi-step operations.
    #[action]
    #[allow(non_snake_case)]
    fn proposeChainedRicardian(
        actions: Vec<Action>,
        auto_exec: bool,
        ricardian_hash: Option<Checksum256>,
        required_witnesses: u8,
        required_domain: Option<String>,
        parent_txid: Option<Checksum256>,
    ) -> u32 {
        ensure_not_rejected(&actions);
        let new_tx = StagedTx::add_with_provenance(
            actions,
            auto_exec,
            ricardian_hash,
            required_witnesses,
            required_domain,
            parent_txid,
        );

        emit_update(new_tx.txid.clone(), StagedTxEvent::PROPOSED);

        accept(new_tx.id, new_tx.txid);

        new_tx.id
    }

    /// Attests to a staged transaction as a qualified witness.
    ///
    /// The witness's `trust_score`, `precision_score`, and `domain_badge` are read from the
    /// credential service (never supplied by the caller) and snapshotted into the attestation.
    /// The witness must:
    /// * not be the proposer or a party to the staged actions,
    /// * hold a trust score of at least `MIN_WITNESS_TRUST_SCORE`,
    /// * hold a domain precision score of at least `MIN_WITNESS_PRECISION_SCORE`, and
    /// * hold an active `domain_badge`, matching `required_domain` if one was set.
    #[action]
    #[allow(non_snake_case)]
    fn attestWitness(id: u32, txid: Checksum256, domain_badge: String) {
        let staged_tx = StagedTx::get(id, txid);
        assert!(staged_tx.required_witnesses > 0, "This staged tx does not require witnesses");
        let witness = get_sender();

        let credentials = Credentials::call();
        let has_badge = credentials.hasDomainBadge(witness, domain_badge.clone());
        let trust_score = credentials.trustScoreOf(witness);
        let precision_score = credentials.precisionScoreOf(witness, domain_badge.clone());
        let parties: Vec<AccountNumber> = staged_tx
            .action_list
            .actions
            .iter()
            .map(|a| a.sender)
            .collect();

        validate_witness_eligibility(
            witness,
            staged_tx.proposer,
            &parties,
            staged_tx.required_domain.as_deref(),
            &domain_badge,
            has_badge,
            trust_score,
            precision_score,
        );

        let table = WitnessAttestationTable::new();
        assert!(
            table.get_index_pk().get(&(id, witness)).is_none(),
            "Witness has already attested to this staged tx"
        );

        let attestation = WitnessAttestation {
            id,
            witness,
            trust_score,
            precision_score,
            domain_badge,
            timestamp: Transact::call().currentBlock().time,
        };
        table.put(&attestation).unwrap();

        try_auto_execute(staged_tx);
    }

    /// Indicates that the caller accepts the specified staged transaction
    ///
    /// Depending on the staging rules enforced by the auth service of the sender
    /// of the staged transaction, this could result in the execution of the transaction.
    ///
    /// * `id`: The ID of the database record containing the staged transaction
    /// * `txid`: The unique txid of the staged transaction
    #[action]
    fn accept(id: u32, txid: Checksum256) {
        let staged_tx = StagedTx::get(id, txid);

        staged_tx.accept();

        emit_update(staged_tx.txid.clone(), StagedTxEvent::ACCEPTED);

        try_auto_execute(staged_tx);
    }

    /// Indicates that the caller rejects the staged transaction
    ///
    /// * `id`: The ID of the database record containing the staged transaction
    /// * `txid`: The unique txid of the staged transaction
    #[action]
    fn reject(id: u32, txid: Checksum256) {
        let staged_tx = StagedTx::get(id, txid);

        staged_tx.reject();

        emit_update(staged_tx.txid.clone(), StagedTxEvent::REJECTED);

        let rejected = staged_tx
            .invocations()
            .iter()
            .any(|(sender, service_method)| {
                StagedTxPolicy::new(*sender).map_or(false, |policy| {
                    policy.does_reject(staged_tx.rejecters(), *service_method)
                })
            });

        if rejected {
            staged_tx.delete();
            emit_update(staged_tx.txid, StagedTxEvent::DELETED);
        }
    }

    /// Removes (deletes) a staged transaction
    ///
    /// A staged transaction can only be removed by its proposer.
    ///
    /// * `id`: The ID of the database record containing the staged transaction
    /// * `txid`: The unique txid of the staged transaction
    #[action]
    fn remove(id: u32, txid: Checksum256) {
        let staged_tx = StagedTx::get(id, txid);

        assert_eq!(
            staged_tx.proposer,
            get_sender(),
            "only the proposer can explicitly remove a staged transaction",
        );

        staged_tx.delete();
        emit_update(staged_tx.txid, StagedTxEvent::DELETED);
    }

    fn execute_impl(staged_tx: StagedTx) {
        debug_print("Executing staged tx\n");

        assert!(
            staged_tx.can_witness_execute(),
            "Staged transaction lacks required witness quorum ({} of {})",
            staged_tx.witness_count(),
            staged_tx.required_witnesses
        );

        let accepters = staged_tx.accepters();
        staged_tx.archive_and_delete(Transact::call().currentBlock().time);

        staged_tx
            .action_list
            .actions
            .into_iter()
            .for_each(|action| {
                debug_print(&format!(
                    "Executing action: {}@{}:{}\n",
                    &action.sender.to_string(),
                    &action.service.to_string(),
                    &action.method.to_string()
                ));

                if !StagedTxPolicy::new(action.sender)
                    .unwrap_or_else(|| {
                        abort_message(&format!("account {} does not exist", action.sender))
                    })
                    .does_auth(
                        accepters.clone(),
                        ServiceMethod::new(action.service, action.method),
                    )
                {
                    abort_message(&format!("Authorization for {} failed", action.sender));
                }
                let act = action.packed();
                unsafe { native_raw::call(act.as_ptr(), act.len() as u32, 0) };
            });

        emit_update(staged_tx.txid.clone(), StagedTxEvent::EXECUTED);
        emit_update(staged_tx.txid, StagedTxEvent::DELETED);
    }

    /// Executes a transaction
    ///
    /// This is only needed when automatic execution is disabled
    ///
    /// * `id`: The ID of the database record containing the staged transaction
    /// * `txid`: The unique txid of the staged transaction
    #[action]
    fn execute(id: u32, txid: Checksum256) {
        let staged_tx = StagedTx::get(id, txid);
        execute_impl(staged_tx);
    }

    /// Gets a staged transaction by id.
    ///
    /// * `id`: The ID of the database record containing the staged transaction
    #[action]
    fn get_staged_tx(id: u32) -> StagedTx {
        StagedTxTable::new()
            .get_index_pk()
            .get(&id)
            .expect("Unknown staged tx")
    }

    /// Registers a zero-PII `PersonalRemittanceAttestation` Ricardian record (OFAC GL 6 / Non-Commercial P2P)
    /// gated by a `witness_quorum >= 2` of soulbound `"humanitarian-verifier"` witnesses.
    #[action]
    #[allow(non_snake_case)]
    fn proposeRemittanceAttestation(
        recipient_commitment_hash: Checksum256,
        corridor_jurisdiction: String,
        non_commercial_personal_declaration_hash: Checksum256,
        asnaf_category: Option<AsnafCategory>,
        witness_quorum: u8,
    ) -> u64 {
        let attestation_id = RemittanceIdRow::next_id();
        let proposer = get_sender();
        let created_at = Transact::call().currentBlock().time;

        let record = build_remittance_attestation(
            attestation_id,
            proposer,
            recipient_commitment_hash.clone(),
            corridor_jurisdiction,
            non_commercial_personal_declaration_hash,
            asnaf_category,
            witness_quorum,
            created_at,
        );

        RemittanceAttestationTable::new().put(&record).unwrap();
        emit_update(recipient_commitment_hash, StagedTxEvent::PROPOSED);

        attestation_id
    }

    /// Co-signs a `PersonalRemittanceAttestation` as an independent witness holding an active
    /// soulbound `domain_badge` (`"humanitarian-verifier"`) and `trust_score >= 8000`.
    #[action]
    #[allow(non_snake_case)]
    fn attestBeneficiaryCommitment(
        attestation_id: u64,
        recipient_commitment_hash: Checksum256,
        domain_badge: String,
    ) {
        let existing = RemittanceAttestationTable::new()
            .get_index_pk()
            .get(&attestation_id)
            .expect("Unknown PersonalRemittanceAttestation");
        let witness = get_sender();

        let credentials = Credentials::call();
        let has_badge = credentials.hasDomainBadge(witness, domain_badge.clone());
        let trust_score = credentials.trustScoreOf(witness);
        let precision_score = credentials.precisionScoreOf(witness, domain_badge.clone());

        let updated_record = apply_remittance_witness_attestation(
            existing,
            witness,
            &recipient_commitment_hash,
            &domain_badge,
            has_badge,
            trust_score,
            precision_score,
        );

        let quorum_met = updated_record.quorum_satisfied;
        RemittanceAttestationTable::new()
            .put(&updated_record)
            .unwrap();

        emit_update(
            recipient_commitment_hash,
            if quorum_met {
                StagedTxEvent::EXECUTED
            } else {
                StagedTxEvent::ACCEPTED
            },
        );
    }

    /// Enforces `witness_quorum >= 2` for `ExternalVault` before settlement execution or confirmation.
    /// Aborts state execution if quorum is unmet or commitment hash mismatches.
    #[action]
    #[allow(non_snake_case)]
    fn verifyRemittanceQuorum(
        attestation_id: u64,
        recipient_commitment_hash: Checksum256,
    ) -> bool {
        let attestation = RemittanceAttestationTable::new()
            .get_index_pk()
            .get(&attestation_id)
            .expect("Unknown PersonalRemittanceAttestation");

        assert_remittance_quorum_satisfied(&attestation, &recipient_commitment_hash);
        true
    }

    /// Fetches a `PersonalRemittanceAttestation` by ID.
    #[action]
    #[allow(non_snake_case)]
    fn getRemittanceAttestation(attestation_id: u64) -> PersonalRemittanceAttestation {
        RemittanceAttestationTable::new()
            .get_index_pk()
            .get(&attestation_id)
            .expect("Unknown PersonalRemittanceAttestation")
    }

    fn emit_update(txid: Checksum256, event_type: u8) {
        Wrapper::emit().history().updated(
            txid,
            get_sender(),
            Transact::call().currentBlock().time,
            StagedTxEvent::from(event_type).to_string(),
        );
    }

    #[event(history)]
    pub fn updated(
        txid: Checksum256,       // The txid of the staged transaction
        actor: AccountNumber,    // The sender of the action causing the event
        datetime: TimePointUSec, // The time of the event emission
        event_type: String,      // The type of event
    ) {
    }
}

#[cfg(test)]
mod tests;
