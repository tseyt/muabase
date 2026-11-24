//! Shared institutional primitives reused across Muabase domain services:
//! - Double-entry append-only ledger conservation & `dwal_merkle`-compatible receipt leaf digests
//! - Scholar/auditor `ReviewerAttestation` & deterministic compliance policy hooks
//! - Pluggable 2-of-3 `TahkimPanel` dispute resolution
//! - Salted SHA-256 commitment + view-key selective-disclosure encrypted envelopes

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn from_hex(hex: &str) -> Result<Vec<u8>, String> {
    let clean = hex
        .strip_prefix("0x")
        .or_else(|| hex.strip_prefix("0X"))
        .unwrap_or(hex);
    if clean.len() % 2 != 0 {
        return Err("Hex string must have even length".into());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for pair in clean.as_bytes().chunks_exact(2) {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + (b - b'a')),
        b'A'..=b'F' => Ok(10 + (b - b'A')),
        _ => Err(format!("Invalid hex character: {}", b as char)),
    }
}

/// Computes the exact `dwal_merkle` leaf entry hash (domain tag `0x01`, LE lengths, `ValueTag::Bytes = 0`).
pub fn dwal_merkle_leaf_hash(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x01u8]);
    h.update((key.len() as u32).to_le_bytes());
    h.update(key);
    h.update([0x00u8]); // ValueTag::Bytes
    h.update((value.len() as u32).to_le_bytes());
    h.update(value);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

// ============================================================================
// 1. Double-Entry Append-Only Ledger & Audit Receipt Primitives
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLine {
    pub account: String,
    /// Positive = credit, Negative = debit (in minor units).
    pub delta: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub entry_id: u64,
    pub category: String,
    pub lines: Vec<JournalLine>,
    pub commitment_hash: [u8; 32],
}

impl JournalEntry {
    /// Enforces double-entry conservation: non-empty lines, non-zero legs, and sum(delta) == 0.
    pub fn validate_balanced(&self) -> Result<(), String> {
        if self.lines.len() < 2 {
            return Err("Double-entry journal requires at least 2 lines".into());
        }
        let mut sum: i128 = 0;
        let mut has_nonzero = false;
        for line in &self.lines {
            if line.account.trim().is_empty() {
                return Err("Journal line account cannot be empty".into());
            }
            if line.delta != 0 {
                has_nonzero = true;
            }
            sum += line.delta as i128;
        }
        if !has_nonzero {
            return Err("Journal entry must have non-zero movement".into());
        }
        if sum != 0 {
            return Err(format!(
                "Conservation violation: double-entry journal sum is {} (must be 0)",
                sum
            ));
        }
        Ok(())
    }

    /// Computes a deterministic receipt leaf hash aligned with `muabase_db::Merkleizer::hash_entry`.
    pub fn receipt_leaf_hash(&self, key_prefix: &str) -> Result<[u8; 32], String> {
        self.validate_balanced()?;
        let key = format!("{}/{}", key_prefix, self.entry_id);
        let value = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(dwal_merkle_leaf_hash(key.as_bytes(), &value))
    }
}

// ============================================================================
// 2. Scholar/Auditor Reviewer Attestation & Compliance Policy Hooks
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewerAttestation {
    pub reviewer_account: String,
    pub template_hash: [u8; 32],
    pub certificate_hash: [u8; 32],
    pub standard_ref: String,
    pub revoked: bool,
}

impl ReviewerAttestation {
    pub fn validate_active(&self, expected_template_hash: &[u8; 32]) -> Result<(), String> {
        if self.revoked {
            return Err("Reviewer attestation has been revoked".into());
        }
        if self.reviewer_account.trim().is_empty() {
            return Err("Reviewer account is empty".into());
        }
        if self.template_hash == [0u8; 32] || self.certificate_hash == [0u8; 32] {
            return Err("Attestation hashes must be non-zero 32-byte SHA-256 digests".into());
        }
        if &self.template_hash != expected_template_hash {
            return Err("Template hash mismatch against scholar attestation".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompliancePolicy {
    pub max_single_disbursement: u64,
    pub allowed_categories: Vec<String>,
    pub allowlist_recipients: Vec<String>,
    pub require_zero_fee_tamlik: bool,
}

impl CompliancePolicy {
    pub fn validate_disbursement(
        &self,
        recipient: &str,
        category: &str,
        amount: u64,
        fee_deducted: u64,
    ) -> Result<(), String> {
        if amount == 0 {
            return Err("Disbursement amount must be positive".into());
        }
        if self.require_zero_fee_tamlik && fee_deducted > 0 {
            return Err("Tamlik Violation: fee_deducted must be 0".into());
        }
        if self.max_single_disbursement > 0 && amount > self.max_single_disbursement {
            return Err(format!(
                "Policy cap exceeded: {} > {}",
                amount, self.max_single_disbursement
            ));
        }
        if !self.allowed_categories.is_empty()
            && !self.allowed_categories.iter().any(|c| c == category)
        {
            return Err(format!("Shurut violation: category '{}' is not allowed", category));
        }
        if !self.allowlist_recipients.is_empty()
            && !self.allowlist_recipients.iter().any(|r| r == recipient)
        {
            return Err(format!("Recipient '{}' is not on policy allowlist", recipient));
        }
        Ok(())
    }
}

// ============================================================================
// 3. Pluggable 2-of-3 Tahkim Arbitration Panel
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TahkimRuling {
    AwardCounterparty,
    RefundFunder,
    Split {
        funder_amount: u64,
        counterparty_amount: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TahkimVote {
    pub arbitrator: String,
    pub ruling: TahkimRuling,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TahkimPanel {
    pub arbitrators: [String; 3],
    pub votes: Vec<TahkimVote>,
}

impl TahkimPanel {
    pub fn new(arbitrators: [String; 3]) -> Result<Self, String> {
        if arbitrators.iter().any(|a| a.trim().is_empty()) {
            return Err("All 3 tahkim arbitrators must be non-empty".into());
        }
        if arbitrators[0] == arbitrators[1]
            || arbitrators[0] == arbitrators[2]
            || arbitrators[1] == arbitrators[2]
        {
            return Err("All 3 tahkim arbitrators must be distinct".into());
        }
        Ok(Self {
            arbitrators,
            votes: Vec::new(),
        })
    }

    pub fn cast_vote(
        &mut self,
        arbitrator: &str,
        ruling: TahkimRuling,
        total_escrow: u64,
    ) -> Result<Option<TahkimRuling>, String> {
        if !self.arbitrators.iter().any(|a| a == arbitrator) {
            return Err(format!("'{}' is not on the tahkim arbitrator panel", arbitrator));
        }
        if self.votes.iter().any(|v| v.arbitrator == arbitrator) {
            return Err(format!("Arbitrator '{}' has already voted", arbitrator));
        }
        if let TahkimRuling::Split {
            funder_amount,
            counterparty_amount,
        } = ruling
        {
            if funder_amount.saturating_add(counterparty_amount) != total_escrow {
                return Err("Split ruling must conserve total_escrow exactly".into());
            }
        }
        self.votes.push(TahkimVote {
            arbitrator: arbitrator.to_string(),
            ruling: ruling.clone(),
        });
        let count = self.votes.iter().filter(|v| v.ruling == ruling).count();
        if count >= 2 {
            Ok(Some(ruling))
        } else {
            Ok(None)
        }
    }
}

// ============================================================================
// 4. Selective-Privacy Commitment & View-Key Encrypted Envelope
// ============================================================================

/// Computes `SHA256(salt || 0x1F || payload)` matching Muabase's zero-PII commitment format.
pub fn compute_salted_commitment(salt: &[u8; 32], payload: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update([0x1fu8]);
    h.update(payload);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn keystream_xor(key: &[u8; 32], nonce: &[u8; 16], input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    for (idx, chunk) in input.chunks(32).enumerate() {
        let mut h = Sha256::new();
        h.update(b"muabase.privacy.ctr.v1");
        h.update(key);
        h.update(nonce);
        h.update((idx as u32).to_le_bytes());
        let block = h.finalize();
        for (i, &b) in chunk.iter().enumerate() {
            out.push(b ^ block[i]);
        }
    }
    out
}

fn compute_auth_tag(key: &[u8; 32], nonce: &[u8; 16], commitment: &[u8; 32], ciphertext: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts 32-byte key");
    mac.update(b"muabase.privacy.mac.v1");
    mac.update(nonce);
    mac.update(commitment);
    mac.update(ciphertext);
    let res = mac.finalize().into_bytes();
    let mut tag = [0u8; 32];
    tag.copy_from_slice(&res);
    tag
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewKeyGrant {
    pub viewer_id: String,
    pub wrapped_key_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectiveDisclosureEnvelope {
    pub version: String,
    pub commitment_hex: String,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
    pub auth_tag_hex: String,
    pub grants: Vec<ViewKeyGrant>,
}

impl SelectiveDisclosureEnvelope {
    /// Seals plaintext PII/terms under `content_key`, binding the on-chain salted commitment
    /// and wrapping `content_key` individually for each authorized viewer (beneficiary/auditor).
    pub fn seal(
        plaintext: &[u8],
        salt: &[u8; 32],
        content_key: &[u8; 32],
        nonce: &[u8; 16],
        viewer_secrets: &[(&str, [u8; 32])],
    ) -> Self {
        let commitment = compute_salted_commitment(salt, plaintext);
        let mut framed = Vec::with_capacity(32 + plaintext.len());
        framed.extend_from_slice(salt);
        framed.extend_from_slice(plaintext);

        let ciphertext = keystream_xor(content_key, nonce, &framed);
        let tag = compute_auth_tag(content_key, nonce, &commitment, &ciphertext);

        let grants = viewer_secrets
            .iter()
            .map(|(viewer_id, secret)| {
                let wrap_mask = sha256_bytes(&[&b"muabase.viewkey.wrap.v1"[..], secret, nonce].concat());
                let mut wrapped = [0u8; 32];
                for i in 0..32 {
                    wrapped[i] = content_key[i] ^ wrap_mask[i];
                }
                ViewKeyGrant {
                    viewer_id: (*viewer_id).to_string(),
                    wrapped_key_hex: to_hex(&wrapped),
                }
            })
            .collect();

        Self {
            version: "muabase.privacy.v1".to_string(),
            commitment_hex: to_hex(&commitment),
            nonce_hex: to_hex(nonce),
            ciphertext_hex: to_hex(&ciphertext),
            auth_tag_hex: to_hex(&tag),
            grants,
        }
    }

    /// Selectively opens the envelope for an authorized viewer and verifies both HMAC integrity
    /// and exact commitment preimage match against the on-chain `commitment_hex`.
    pub fn open_for_viewer(&self, viewer_id: &str, viewer_secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        if self.version != "muabase.privacy.v1" {
            return Err("Unsupported envelope version".into());
        }
        let grant = self
            .grants
            .iter()
            .find(|g| g.viewer_id == viewer_id)
            .ok_or_else(|| format!("Viewer '{}' has no selective disclosure grant", viewer_id))?;

        let nonce_vec = from_hex(&self.nonce_hex)?;
        let nonce: [u8; 16] = nonce_vec
            .try_into()
            .map_err(|_| "Invalid 16-byte nonce".to_string())?;
        let commitment_vec = from_hex(&self.commitment_hex)?;
        let commitment: [u8; 32] = commitment_vec
            .try_into()
            .map_err(|_| "Invalid 32-byte commitment".to_string())?;
        let expected_tag_vec = from_hex(&self.auth_tag_hex)?;
        let expected_tag: [u8; 32] = expected_tag_vec
            .try_into()
            .map_err(|_| "Invalid 32-byte auth tag".to_string())?;
        let wrapped_vec = from_hex(&grant.wrapped_key_hex)?;
        let wrapped: [u8; 32] = wrapped_vec
            .try_into()
            .map_err(|_| "Invalid wrapped key".to_string())?;
        let ciphertext = from_hex(&self.ciphertext_hex)?;

        let wrap_mask = sha256_bytes(&[&b"muabase.viewkey.wrap.v1"[..], viewer_secret, &nonce].concat());
        let mut content_key = [0u8; 32];
        for i in 0..32 {
            content_key[i] = wrapped[i] ^ wrap_mask[i];
        }

        let actual_tag = compute_auth_tag(&content_key, &nonce, &commitment, &ciphertext);
        if actual_tag != expected_tag {
            return Err("Authentication failed: invalid view key or tampered ciphertext".into());
        }

        let framed = keystream_xor(&content_key, &nonce, &ciphertext);
        if framed.len() < 32 {
            return Err("Truncated plaintext frame".into());
        }
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&framed[..32]);
        let plaintext = framed[32..].to_vec();

        if compute_salted_commitment(&salt, &plaintext) != commitment {
            return Err("Commitment mismatch after decryption".into());
        }
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_double_entry_journal_when_balanced_then_conserves_and_emits_dwal_receipt_hash() {
        let entry = JournalEntry {
            entry_id: 7,
            category: "zakat_fuqara".into(),
            lines: vec![
                JournalLine { account: "corridor-pool".into(), delta: -100_000 },
                JournalLine { account: "bridge-tr".into(), delta: 100_000 },
            ],
            commitment_hash: [0x11; 32],
        };
        assert!(entry.validate_balanced().is_ok());
        let h1 = entry.receipt_leaf_hash("receipt/corridor").unwrap();
        let h2 = entry.receipt_leaf_hash("receipt/corridor").unwrap();
        assert_eq!(h1, h2);
        assert_ne!(h1, [0u8; 32]);

        let unbalanced = JournalEntry {
            entry_id: 8,
            category: "zakat_fuqara".into(),
            lines: vec![
                JournalLine { account: "corridor-pool".into(), delta: -100_000 },
                JournalLine { account: "bridge-tr".into(), delta: 99_000 },
            ],
            commitment_hash: [0x11; 32],
        };
        assert!(unbalanced.validate_balanced().is_err());
    }

    #[test]
    fn given_compliance_policy_and_scholar_attestation_when_enforced_then_violations_fail_deterministically() {
        let tpl = sha256_bytes(b"istisna-milestone-v1");
        let att = ReviewerAttestation {
            reviewer_account: "shariah-brd".into(),
            template_hash: tpl,
            certificate_hash: sha256_bytes(b"aaoifi-cert-2026"),
            standard_ref: "AAOIFI-11".into(),
            revoked: false,
        };
        assert!(att.validate_active(&tpl).is_ok());
        assert!(att.validate_active(&[0xaa; 32]).is_err());

        let policy = CompliancePolicy {
            max_single_disbursement: 500_000,
            allowed_categories: vec!["scholarship".into(), "utility".into()],
            allowlist_recipients: vec!["masjid-ops".into()],
            require_zero_fee_tamlik: true,
        };
        assert!(policy.validate_disbursement("masjid-ops", "scholarship", 250_000, 0).is_ok());
        assert!(policy.validate_disbursement("masjid-ops", "scholarship", 250_000, 1).is_err());
        assert!(policy.validate_disbursement("masjid-ops", "unapproved", 10_000, 0).is_err());
        assert!(policy.validate_disbursement("masjid-ops", "utility", 600_000, 0).is_err());
    }

    #[test]
    fn given_tahkim_panel_when_two_of_three_agree_then_ruling_resolves_and_conserves_escrow() {
        let mut panel = TahkimPanel::new(["arb1".into(), "arb2".into(), "arb3".into()]).unwrap();
        let split = TahkimRuling::Split {
            funder_amount: 40_000,
            counterparty_amount: 60_000,
        };
        assert_eq!(panel.cast_vote("arb1", split.clone(), 100_000).unwrap(), None);
        assert!(panel.cast_vote("arb1", split.clone(), 100_000).is_err());
        assert!(panel.cast_vote("outsider", split.clone(), 100_000).is_err());
        assert_eq!(
            panel.cast_vote("arb2", split.clone(), 100_000).unwrap(),
            Some(split)
        );
    }

    #[test]
    fn given_selective_disclosure_envelope_when_opened_with_view_key_then_pii_matches_commitment_and_unauthorized_fails() {
        let pii = br#"{"beneficiary":"Amina Q.","iban":"TR0012345"}"#;
        let salt = [0x42u8; 32];
        let content_key = [0x77u8; 32];
        let nonce = [0x99u8; 16];
        let auditor_secret = [0x12u8; 32];
        let wrong_secret = [0x34u8; 32];

        let env = SelectiveDisclosureEnvelope::seal(
            pii,
            &salt,
            &content_key,
            &nonce,
            &[("auditor-1", auditor_secret)],
        );
        // Plaintext must not appear in serialized on-chain envelope
        let serialized = serde_json::to_string(&env).unwrap();
        assert!(!serialized.contains("Amina"));
        assert!(!serialized.contains("TR0012345"));

        // Authorized viewer decrypts and verifies commitment
        let opened = env.open_for_viewer("auditor-1", &auditor_secret).unwrap();
        assert_eq!(opened, pii);

        // Wrong secret or ungranted viewer fails
        assert!(env.open_for_viewer("auditor-1", &wrong_secret).is_err());
        assert!(env.open_for_viewer("random-user", &auditor_secret).is_err());
    }
}
