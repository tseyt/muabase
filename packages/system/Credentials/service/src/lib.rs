mod tables;

pub const CRED_SYS: psibase::AccountNumber = psibase::account!("cred-sys");

#[psibase::service(name = "credential", tables = "tables::tables")]
pub mod service {
    use crate::tables::tables::*;
    use crate::CRED_SYS;
    use psibase::services::{
        tokens::{Quantity, Wrapper as Tokens},
        transact::{ServiceMethod, Wrapper as Transact},
        virtual_server::Wrapper as VirtualServer,
    };
    use psibase::*;

    const VSERVER: AccountNumber = VirtualServer::SERVICE;

    /// Initial `precision_score` (Dabt, in bps) assigned when a domain badge is first issued.
    pub const DEFAULT_INITIAL_PRECISION_SCORE: u32 = 8500;

    #[action]
    fn init() {
        InitRow::init();
    }

    #[pre_action(exclude(init))]
    fn check_init() {
        let table = InitTable::read();
        assert!(
            table.get_index_pk().get(&()).is_some(),
            "service not inited",
        );
    }

    #[action]
    #[allow(non_snake_case)]
    fn canAuthUserSys(user: AccountNumber) {
        assert_eq!(
            user, CRED_SYS,
            "only {CRED_SYS} can use credentials as an auth service",
        );
    }

    fn now() -> TimePointSec {
        let native = KvHandle::new(psibase::DbId::Native, &[], KvMode::Read);
        let status = kv_get::<psibase::StatusRow, _>(&native, &psibase::status_key())
            .unwrap()
            .unwrap();

        status.current.time.seconds()
    }

    #[action]
    #[allow(non_snake_case)]
    fn checkAuthSys(
        flags: u32,
        sender: AccountNumber,
        action: ServiceMethod,
        claims: Vec<Claim>,
    ) -> bool {
        use psibase::services::transact::auth_interface::*;

        assert_eq!(sender, CRED_SYS, "sender must be {}", CRED_SYS);

        assert_eq!(claims.len(), 1, "Must be exactly one claim");
        let claim = &claims[0];

        let credential = CredentialTable::read()
            .get_index_by_pkh()
            .get(&psibase::sha256(&claim.rawData))
            .expect("Claim uses an invalid credential");

        // FIRST_AUTH_FLAG is set when this check is running for the top-level action and
        // we are NOT executing speculatively.
        if (flags & FIRST_AUTH_FLAG) != 0
            && VirtualServer::call().is_billing_enabled()
            && get_sender() == psibase::services::transact::SERVICE
        {
            VirtualServer::call_as(CRED_SYS).bill_to_sub(credential.id.to_string());
        }

        assert_eq!(
            claim.service,
            psibase::services::verify_sig::SERVICE,
            "Claim must use verify-sig",
        );

        assert_eq!(
            action.service, credential.issuer,
            "Can only call actions on the credential issuer service",
        );
        assert!(
            credential.allowed_actions.contains(&action.method),
            "Action {} not allowed using this credential",
            action.method
        );

        assert!(
            credential.expiry_date.is_none() || credential.expiry_date.unwrap() > now(),
            "Credential expired",
        );
        true
    }

    #[action]
    #[allow(non_snake_case)]
    fn getDlgsSys(_sender: AccountNumber) -> Vec<AccountNumber> {
        Vec::new()
    }

    #[action]
    #[allow(non_snake_case)]
    fn isAuthSys(_sender: AccountNumber, _authorizers: Vec<AccountNumber>) -> bool {
        abort_message("isAuthSys not supported");
    }

    #[action]
    #[allow(non_snake_case)]
    fn isRejectSys(_sender: AccountNumber, _rejecters: Vec<AccountNumber>) -> bool {
        abort_message("isRejectSys not supported");
    }

    /// Issues a credential
    ///
    /// Parameters:
    /// - `pubkey_fingerprint`: The fingerprint of the credential public key
    /// - `expires`: The number of seconds until the credential expires
    /// - `allowed_actions`: The actions that the credential is allowed to call on the issuer service
    ///
    /// This action is meant to be called inline by another service.
    /// The caller service is the credential issuer.
    ///
    /// A transaction sent from the CREDENTIAL_SENDER account must include a proof for a claim
    /// that matches the specified public key.
    #[action]
    fn issue(
        pubkey_fingerprint: Checksum256,
        expires: Option<u32>,
        allowed_actions: Vec<MethodNumber>,
    ) -> u32 {
        Credential::add(pubkey_fingerprint, expires, allowed_actions)
    }

    /// Notifies the credentials service that tokens have been credited to a credential
    ///
    /// This notification must be called after crediting the credential's service, or else
    /// the credited tokens will not be aplied to a particular credential.
    #[action]
    fn resource(id: u32, amount: Quantity) {
        let credential = CredentialTable::read()
            .get_index_pk()
            .get(&id)
            .expect("Credential not found");

        assert_eq!(
            credential.issuer,
            get_sender(),
            "Only the issuer can credit this credential",
        );
        let sys = Tokens::call()
            .getSysToken()
            .expect("System token not found")
            .id;

        Tokens::call().debit(sys, get_sender(), amount, "".into());
        Tokens::call().credit(sys, CRED_SYS, amount, "".into());
        Tokens::call_as(CRED_SYS).debit(sys, SERVICE, amount, "".into());
        Tokens::call_as(CRED_SYS).credit(sys, VSERVER, amount, "".into());
        VirtualServer::call_as(CRED_SYS).buy_res_sub(amount, id.to_string());
    }

    /// Gets the fingerprint of the specified credential pubkey
    #[allow(non_snake_case)]
    #[action]
    fn getFingerprint(id: u32) -> Checksum256 {
        CredentialTable::read()
            .get_index_pk()
            .get(&id)
            .expect("Credential DNE")
            .pubkey_fingerprint
    }

    /// Gets the `expiry_date` of the specified credential
    #[action]
    fn get_expiry_date(id: u32) -> Option<TimePointSec> {
        CredentialTable::read()
            .get_index_pk()
            .get(&id)
            .expect("Credential DNE")
            .expiry_date
    }

    /// Gets the `id` of the active credential
    #[action]
    fn get_active() -> Option<u32> {
        let claims = Transact::call().getTransaction().claims;
        let table = CredentialTable::read().get_index_by_pkh();

        let mut active_id = None;
        let now = Transact::call().currentBlock().time.seconds();
        for claim in claims {
            if let Some(credential) = table.get(&psibase::sha256(&claim.rawData)) {
                assert!(
                    active_id.is_none(),
                    "Only one active credential is supported",
                );
                assert!(
                    credential.expiry_date.is_none() || credential.expiry_date.unwrap() > now,
                    "Credential expired",
                );
                active_id = Some(credential.id);
            }
        }

        active_id
    }

    /// Deletes the specified credential.
    /// Can only be called by the credential's issuer.
    #[action]
    fn consume(id: u32) {
        let table = CredentialTable::read_write();

        let credential = table.get_index_pk().get(&id).expect("Credential DNE");
        assert_eq!(credential.issuer, get_sender(), "Unauthorized");
        table.remove(&credential);

        // Avoid orphaned resources
        VirtualServer::call_as(CRED_SYS).del_res_sub(id.to_string());
    }

    /// Updates or initializes a soulbound trust score (Adalah) reliability record
    #[action]
    #[allow(non_snake_case)]
    fn updateTrustScore(account: AccountNumber, score: u32, is_success: bool) {
        validate_trust_score_update(get_sender(), account, score);

        let table = TrustScoreTable::read_write();
        let current = table.get_index_pk().get(&account);
        let record = apply_trust_score_update(current, account, score, is_success, now());

        table.put(&record).unwrap();
    }

    /// Backwards-compatible alias for updateTrustScore
    #[action]
    #[allow(non_snake_case)]
    fn updateAdalahScore(account: AccountNumber, score: u32, is_success: bool) {
        updateTrustScore(account, score, is_success);
    }

    /// Fetches the trust score reliability record for an account
    #[action]
    #[allow(non_snake_case)]
    fn getTrustScore(account: AccountNumber) -> Option<TrustScoreRecord> {
        TrustScoreTable::read().get_index_pk().get(&account)
    }

    /// Backwards-compatible alias for getTrustScore
    #[action]
    #[allow(non_snake_case)]
    fn getAdalahRecord(account: AccountNumber) -> Option<TrustScoreRecord> {
        getTrustScore(account)
    }

    /// Returns the account's current trust score, or 0 if it has no record.
    /// Accounts with no audit history are untrusted by default.
    #[action]
    #[allow(non_snake_case)]
    fn trustScoreOf(account: AccountNumber) -> u32 {
        TrustScoreTable::read()
            .get_index_pk()
            .get(&account)
            .map(|r| r.score)
            .unwrap_or(0)
    }

    /// Issues a non-transferable domain competence badge (Ikhtisas)
    #[action]
    #[allow(non_snake_case)]
    fn issueDomainBadge(account: AccountNumber, domain: String, cert_hash: Checksum256) {
        validate_domain_badge_issuance(get_sender(), account, &domain);
        let table = DomainBadgeTable::read_write();
        let record = DomainBadgeRecord {
            account,
            domain: domain.clone(),
            issuer: get_sender(),
            issued_at: now(),
            cert_hash,
            active: true,
            precision_score: DEFAULT_INITIAL_PRECISION_SCORE,
            accurate_deliveries: 0,
            regressions_recorded: 0,
        };
        table.put(&record).unwrap();
    }

    /// Backwards-compatible alias for issueDomainBadge
    #[action]
    #[allow(non_snake_case)]
    fn issueIkhtisasTag(account: AccountNumber, domain: String, cert_hash: Checksum256) {
        issueDomainBadge(account, domain, cert_hash);
    }

    /// Updates the domain-scoped execution accuracy score (precision_score / Dabt) on an active badge
    #[action]
    #[allow(non_snake_case)]
    fn updatePrecisionScore(
        account: AccountNumber,
        domain: String,
        score: u32,
        is_accurate: bool,
    ) {
        let table = DomainBadgeTable::read_write();
        let existing = table
            .get_index_pk()
            .get(&(account, domain.clone()))
            .expect("Domain badge DNE");
        let updated = apply_precision_score_update(get_sender(), existing, score, is_accurate);
        table.put(&updated).unwrap();
    }

    /// Backwards-compatible Muamalat alias for updatePrecisionScore (Dabt)
    #[action]
    #[allow(non_snake_case)]
    fn updateDabtScore(account: AccountNumber, domain: String, score: u32, is_accurate: bool) {
        updatePrecisionScore(account, domain, score, is_accurate);
    }

    /// Returns the account's current domain-scoped precision_score (Dabt), or 0 if inactive/missing
    #[action]
    #[allow(non_snake_case)]
    fn precisionScoreOf(account: AccountNumber, domain: String) -> u32 {
        DomainBadgeTable::read()
            .get_index_pk()
            .get(&(account, domain))
            .filter(|r| r.active)
            .map(|r| r.precision_score)
            .unwrap_or(0)
    }

    /// Backwards-compatible Muamalat alias for precisionScoreOf (Dabt)
    #[action]
    #[allow(non_snake_case)]
    fn dabtScoreOf(account: AccountNumber, domain: String) -> u32 {
        precisionScoreOf(account, domain)
    }

    /// Revokes a domain competence badge
    #[action]
    #[allow(non_snake_case)]
    fn revokeDomainBadge(account: AccountNumber, domain: String) {
        let table = DomainBadgeTable::read_write();
        if let Some(mut record) = table.get_index_pk().get(&(account, domain.clone())) {
            assert!(
                get_sender() == record.issuer || get_sender() == account,
                "Unauthorized to revoke badge"
            );
            record.active = false;
            table.put(&record).unwrap();
        }
    }

    /// Backwards-compatible alias for revokeDomainBadge
    #[action]
    #[allow(non_snake_case)]
    fn revokeIkhtisasTag(account: AccountNumber, domain: String) {
        revokeDomainBadge(account, domain);
    }

    /// Checks whether an account holds an active domain badge
    #[action]
    #[allow(non_snake_case)]
    fn hasDomainBadge(account: AccountNumber, domain: String) -> bool {
        DomainBadgeTable::read()
            .get_index_pk()
            .get(&(account, domain))
            .map(|r| r.active)
            .unwrap_or(false)
    }

    /// Backwards-compatible alias for hasDomainBadge
    #[action]
    #[allow(non_snake_case)]
    fn hasIkhtisas(account: AccountNumber, domain: String) -> bool {
        hasDomainBadge(account, domain)
    }

    pub fn validate_trust_score_update(sender: AccountNumber, account: AccountNumber, score: u32) {
        // A soulbound reputation is only meaningful if it is attested by someone else
        assert!(sender != account, "Cannot report on your own trust score");
        assert!(score <= 10000, "trust_score must be within 0..=10000 bps");
    }

    pub fn apply_trust_score_update(
        existing: Option<TrustScoreRecord>,
        account: AccountNumber,
        score: u32,
        is_success: bool,
        timestamp: TimePointSec,
    ) -> TrustScoreRecord {
        let mut record = existing.unwrap_or(TrustScoreRecord {
            account,
            score: 0,
            total_audits: 0,
            successful_audits: 0,
            disputes_involved: 0,
            last_updated: timestamp,
        });

        record.total_audits += 1;
        if is_success {
            record.successful_audits += 1;
        } else {
            record.disputes_involved += 1;
        }
        record.score = score;
        record.last_updated = timestamp;
        record
    }

    pub fn validate_domain_badge_issuance(
        sender: AccountNumber,
        account: AccountNumber,
        domain: &str,
    ) {
        assert!(sender != account, "Cannot issue a domain badge to yourself");
        assert!(!domain.is_empty(), "Domain must not be empty");
    }

    pub fn apply_precision_score_update(
        sender: AccountNumber,
        mut record: DomainBadgeRecord,
        score: u32,
        is_accurate: bool,
    ) -> DomainBadgeRecord {
        assert!(
            sender != record.account,
            "Cannot report on your own precision score"
        );
        assert!(
            record.active,
            "Cannot update precision_score on an inactive domain badge"
        );
        assert!(
            score <= 10000,
            "precision_score must be within 0..=10000 bps"
        );

        if is_accurate {
            record.accurate_deliveries += 1;
        } else {
            record.regressions_recorded += 1;
        }
        record.precision_score = score;
        record
    }
}

#[cfg(test)]
mod tests {
    use super::service::{
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
