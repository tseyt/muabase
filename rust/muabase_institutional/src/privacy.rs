//! Selective-privacy salted SHA-256 commitment and view-key encrypted envelopes.

use crate::codec::{from_hex, from_hex_fixed, sha256_bytes, to_hex};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub const PRIVACY_ENVELOPE_VERSION: &str = "muabase.privacy.v1";
const COMMITMENT_SEPARATOR: u8 = 0x1f;
const CTR_KEYSTREAM_DOMAIN: &[u8] = b"muabase.privacy.ctr.v1";
const AUTH_MAC_DOMAIN: &[u8] = b"muabase.privacy.mac.v1";
const VIEWKEY_WRAP_DOMAIN: &[u8] = b"muabase.viewkey.wrap.v1";

/// Computes `SHA256(salt || 0x1F || payload)` matching Muabase's zero-PII commitment format.
pub fn compute_salted_commitment(salt: &[u8; 32], payload: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update([COMMITMENT_SEPARATOR]);
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
        h.update(CTR_KEYSTREAM_DOMAIN);
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

fn new_auth_mac(
    key: &[u8; 32],
    nonce: &[u8; 16],
    commitment: &[u8; 32],
    ciphertext: &[u8],
) -> HmacSha256 {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts 32-byte key");
    mac.update(AUTH_MAC_DOMAIN);
    mac.update(nonce);
    mac.update(commitment);
    mac.update(ciphertext);
    mac
}

fn compute_auth_tag(
    key: &[u8; 32],
    nonce: &[u8; 16],
    commitment: &[u8; 32],
    ciphertext: &[u8],
) -> [u8; 32] {
    let res = new_auth_mac(key, nonce, commitment, ciphertext)
        .finalize()
        .into_bytes();
    let mut tag = [0u8; 32];
    tag.copy_from_slice(&res);
    tag
}

fn derive_wrap_mask(viewer_secret: &[u8; 32], nonce: &[u8; 16]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(VIEWKEY_WRAP_DOMAIN.len() + 32 + 16);
    buf.extend_from_slice(VIEWKEY_WRAP_DOMAIN);
    buf.extend_from_slice(viewer_secret);
    buf.extend_from_slice(nonce);
    sha256_bytes(&buf)
}

fn xor32(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = a[i] ^ b[i];
    }
    out
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
                let wrap_mask = derive_wrap_mask(secret, nonce);
                let wrapped = xor32(content_key, &wrap_mask);
                ViewKeyGrant {
                    viewer_id: (*viewer_id).to_string(),
                    wrapped_key_hex: to_hex(&wrapped),
                }
            })
            .collect();

        Self {
            version: PRIVACY_ENVELOPE_VERSION.to_string(),
            commitment_hex: to_hex(&commitment),
            nonce_hex: to_hex(nonce),
            ciphertext_hex: to_hex(&ciphertext),
            auth_tag_hex: to_hex(&tag),
            grants,
        }
    }

    /// Selectively opens the envelope for an authorized viewer and verifies both constant-time
    /// HMAC integrity and exact commitment preimage match against the on-chain `commitment_hex`.
    pub fn open_for_viewer(
        &self,
        viewer_id: &str,
        viewer_secret: &[u8; 32],
    ) -> Result<Vec<u8>, String> {
        if self.version != PRIVACY_ENVELOPE_VERSION {
            return Err("Unsupported envelope version".into());
        }
        let grant = self
            .grants
            .iter()
            .find(|g| g.viewer_id == viewer_id)
            .ok_or_else(|| format!("Viewer '{viewer_id}' has no selective disclosure grant"))?;

        let nonce = from_hex_fixed::<16>(&self.nonce_hex, "nonce")?;
        let commitment = from_hex_fixed::<32>(&self.commitment_hex, "commitment")?;
        let expected_tag = from_hex_fixed::<32>(&self.auth_tag_hex, "auth tag")?;
        let wrapped = from_hex_fixed::<32>(&grant.wrapped_key_hex, "wrapped key")?;
        let ciphertext = from_hex(&self.ciphertext_hex)?;

        let wrap_mask = derive_wrap_mask(viewer_secret, &nonce);
        let content_key = xor32(&wrapped, &wrap_mask);

        new_auth_mac(&content_key, &nonce, &commitment, &ciphertext)
            .verify_slice(&expected_tag)
            .map_err(|_| {
                "Authentication failed: invalid view key or tampered ciphertext".to_string()
            })?;

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
