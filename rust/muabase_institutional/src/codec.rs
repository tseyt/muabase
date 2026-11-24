//! Cryptographic hashing, hex encoding, and `dwal_merkle`-compatible leaf digests.

use sha2::{Digest, Sha256};

/// Domain separation tag for `muabase_db::Merkleizer` leaf entries.
pub const DWAL_LEAF_DOMAIN_TAG: u8 = 0x01;
/// Value tag for `muabase_db::ValueTag::Bytes`.
pub const DWAL_VALUE_TAG_BYTES: u8 = 0x00;

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

pub fn from_hex_fixed<const N: usize>(hex: &str, field_name: &str) -> Result<[u8; N], String> {
    let bytes = from_hex(hex)?;
    bytes
        .try_into()
        .map_err(|_| format!("Invalid {N}-byte {field_name}"))
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + (b - b'a')),
        b'A'..=b'F' => Ok(10 + (b - b'A')),
        _ => Err(format!("Invalid hex character: {}", b as char)),
    }
}

/// Computes the exact `dwal_merkle` leaf entry hash (`0x01 || u32le(klen) || key || 0x00 || u32le(vlen) || value`).
pub fn dwal_merkle_leaf_hash(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([DWAL_LEAF_DOMAIN_TAG]);
    h.update((key.len() as u32).to_le_bytes());
    h.update(key);
    h.update([DWAL_VALUE_TAG_BYTES]);
    h.update((value.len() as u32).to_le_bytes());
    h.update(value);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}
