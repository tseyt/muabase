use crate::{self as psibase, account, AccountNumber};
use fracpack::{Pack, ToSchema, Unpack};
use serde::{Deserialize, Serialize};

pub const CREDENTIAL_SENDER: AccountNumber = account!("cred-sys");

/// Upper bound for `trust_score` values (basis points, 10000 = 100.00%)
pub const MAX_TRUST_SCORE: u32 = 10000;

/// Soulbound reliability record (`trust_score` / Adalah) as stored by the credential service
#[derive(Debug, Clone, Pack, Unpack, Serialize, Deserialize, ToSchema)]
#[fracpack(fracpack_mod = "fracpack")]
pub struct TrustScoreRecord {
    pub account: AccountNumber,
    pub score: u32,
    pub total_audits: u32,
    pub successful_audits: u32,
    pub disputes_involved: u32,
    pub last_updated: crate::TimePointSec,
}

#[crate::service(name = "credential", dispatch = false, psibase_mod = "crate")]
#[allow(unused_variables)]
pub mod service {
    use crate::{
        services::{tokens::Quantity, transact::ServiceMethod},
        AccountNumber, Claim, MethodNumber, TimePointSec,
    };

    #[action]
    fn init() {
        unimplemented!()
    }

    #[allow(non_snake_case)]
    #[action]
    fn canAuthUserSys(user: AccountNumber) {
        unimplemented!()
    }

    #[allow(non_snake_case)]
    #[action]
    fn checkAuthSys(
        flags: u32,
        sender: AccountNumber,
        action: ServiceMethod,
        claims: Vec<Claim>,
    ) -> bool {
        unimplemented!()
    }

    #[action]
    #[allow(non_snake_case)]
    fn getDlgsSys(sender: AccountNumber) -> Vec<AccountNumber> {
        unimplemented!()
    }

    #[action]
    #[allow(non_snake_case)]
    fn isAuthSys(sender: AccountNumber, authorizers: Vec<AccountNumber>) -> bool {
        unimplemented!()
    }

    #[action]
    #[allow(non_snake_case)]
    fn isRejectSys(sender: AccountNumber, rejecters: Vec<AccountNumber>) -> bool {
        unimplemented!()
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
        pubkey_fingerprint: crate::Checksum256,
        expires: Option<u32>,
        allowed_actions: Vec<MethodNumber>,
    ) -> u32 {
        unimplemented!()
    }

    /// Notifies the credentials service that tokens have been credited to a credential
    ///
    /// This notification must be called after crediting the credential's service, or else
    /// the credited tokens will not be aplied to a particular credential.
    #[action]
    fn resource(id: u32, amount: Quantity) {
        unimplemented!()
    }

    /// Gets the fingerprint of the specified credential pubkey
    #[allow(non_snake_case)]
    #[action]
    fn getFingerprint(id: u32) -> crate::Checksum256 {
        unimplemented!()
    }

    /// Gets the `expiry_date` of the specified credential
    #[action]
    fn get_expiry_date(id: u32) -> Option<TimePointSec> {
        unimplemented!()
    }

    /// Gets the `id` of the active credential
    #[action]
    fn get_active() -> Option<u32> {
        unimplemented!()
    }

    /// Deletes the specified credential.
    /// Can only be called by the credential's issuer.
    #[action]
    fn consume(id: u32) {
        unimplemented!()
    }

    /// Updates or initializes a soulbound trust score (Adalah) reliability record.
    /// The caller is the reporter and may not report on itself.
    #[action]
    #[allow(non_snake_case)]
    fn updateTrustScore(account: AccountNumber, score: u32, is_success: bool) {
        unimplemented!()
    }

    /// Backwards-compatible alias for updateTrustScore
    #[action]
    #[allow(non_snake_case)]
    fn updateAdalahScore(account: AccountNumber, score: u32, is_success: bool) {
        unimplemented!()
    }

    /// Fetches the trust score reliability record for an account
    #[action]
    #[allow(non_snake_case)]
    fn getTrustScore(account: AccountNumber) -> Option<super::TrustScoreRecord> {
        unimplemented!()
    }

    /// Backwards-compatible alias for getTrustScore
    #[action]
    #[allow(non_snake_case)]
    fn getAdalahRecord(account: AccountNumber) -> Option<super::TrustScoreRecord> {
        unimplemented!()
    }

    /// Returns the account's current trust score, or 0 if it has no record
    #[action]
    #[allow(non_snake_case)]
    fn trustScoreOf(account: AccountNumber) -> u32 {
        unimplemented!()
    }

    /// Issues a non-transferable domain competence badge (Ikhtisas).
    /// The caller is the issuer and may not issue a badge to itself.
    #[action]
    #[allow(non_snake_case)]
    fn issueDomainBadge(account: AccountNumber, domain: String, cert_hash: crate::Checksum256) {
        unimplemented!()
    }

    /// Backwards-compatible alias for issueDomainBadge
    #[action]
    #[allow(non_snake_case)]
    fn issueIkhtisasTag(account: AccountNumber, domain: String, cert_hash: crate::Checksum256) {
        unimplemented!()
    }

    /// Revokes a domain competence badge
    #[action]
    #[allow(non_snake_case)]
    fn revokeDomainBadge(account: AccountNumber, domain: String) {
        unimplemented!()
    }

    /// Backwards-compatible alias for revokeDomainBadge
    #[action]
    #[allow(non_snake_case)]
    fn revokeIkhtisasTag(account: AccountNumber, domain: String) {
        unimplemented!()
    }

    /// Checks whether an account holds an active domain badge
    #[action]
    #[allow(non_snake_case)]
    fn hasDomainBadge(account: AccountNumber, domain: String) -> bool {
        unimplemented!()
    }

    /// Backwards-compatible alias for hasDomainBadge
    #[action]
    #[allow(non_snake_case)]
    fn hasIkhtisas(account: AccountNumber, domain: String) -> bool {
        unimplemented!()
    }
}

#[test]
fn verify_schema() {
    crate::assert_schema_matches_package::<Wrapper>();
}
