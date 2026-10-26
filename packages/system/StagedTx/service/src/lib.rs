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

    const ENABLE_PRINT: bool = false;

    /// Minimum `trust_score` (bps) a witness needs to attest. Matches the documented
    /// adjudicator floor used by Guilds / ClaimCouncil evaluators (80.00%).
    const MIN_WITNESS_TRUST_SCORE: u32 = 8000;

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

    /// Proposes a new staged transaction containing the specified actions.
    /// Returns the ID of the database record containing the staged transaction.
    ///
    /// * `actions`: The actions to be staged
    /// * `auto_exec`: Enables automatic execution as soon as the transaction has enough approvals
    #[action]
    fn propose(actions: Vec<Action>, auto_exec: bool) -> u32 {
        let new_tx = StagedTx::add(actions, auto_exec);

        emit_update(new_tx.txid.clone(), StagedTxEvent::PROPOSED);

        // A proposal is also an implicit accept
        accept(new_tx.id, new_tx.txid);

        new_tx.id
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

    /// Attests to a staged transaction as a qualified witness.
    ///
    /// The witness's `trust_score` and `domain_badge` are read from the credential
    /// service (never supplied by the caller) and snapshotted into the attestation.
    /// The witness must:
    /// * not be the proposer or a party to the staged actions,
    /// * hold a trust score of at least `MIN_WITNESS_TRUST_SCORE`, and
    /// * hold an active `domain_badge`, matching `required_domain` if one was set.
    #[action]
    #[allow(non_snake_case)]
    fn attestWitness(id: u32, txid: Checksum256, domain_badge: String) {
        let staged_tx = StagedTx::get(id, txid);
        assert!(staged_tx.required_witnesses > 0, "This staged tx does not require witnesses");
        let witness = get_sender();

        assert!(witness != staged_tx.proposer, "Proposer cannot witness their own staged tx");
        assert!(
            !staged_tx.action_list.actions.iter().any(|a| a.sender == witness),
            "A party to the staged tx cannot act as its witness"
        );

        if let Some(required) = &staged_tx.required_domain {
            assert!(
                &domain_badge == required,
                "Witness must attest with the required domain badge: {}",
                required
            );
        }

        let credentials = Credentials::call();
        assert!(
            credentials.hasDomainBadge(witness, domain_badge.clone()),
            "Witness does not hold an active domain badge: {}",
            domain_badge
        );
        let trust_score = credentials.trustScoreOf(witness);
        assert!(
            trust_score >= MIN_WITNESS_TRUST_SCORE,
            "Witness trust_score {} is below the required {}",
            trust_score,
            MIN_WITNESS_TRUST_SCORE
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
            domain_badge,
            timestamp: Transact::call().currentBlock().time,
        };
        table.put(&attestation).unwrap();

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

        if staged_tx.auto_exec && staged_tx.can_witness_execute() {
            let authorized = staged_tx
                .invocations()
                .iter()
                .all(|(sender, service_method)| {
                    StagedTxPolicy::new(*sender).map_or(true, |policy| {
                        policy.does_auth(staged_tx.accepters(), *service_method)
                    })
                });

            debug_print(&format!("authorized: {}\n", authorized.to_string()));

            if authorized {
                execute_impl(staged_tx);
            }
        }
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
        staged_tx.delete();

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
