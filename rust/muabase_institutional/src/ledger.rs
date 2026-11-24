//! Double-entry append-only ledger conservation and `dwal_merkle` audit receipt digests.

use crate::codec::dwal_merkle_leaf_hash;
use crate::validation::validate_non_zero_digest;
use serde::{Deserialize, Serialize};

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
    /// Constructs a checked, balanced 2-leg double-entry transfer (`-amount` / `+amount`).
    pub fn transfer(
        entry_id: u64,
        category: impl Into<String>,
        from_account: impl Into<String>,
        to_account: impl Into<String>,
        amount: u64,
        commitment_hash: [u8; 32],
    ) -> Result<Self, String> {
        Self::fan_out(
            entry_id,
            category,
            from_account,
            &[(to_account.into(), amount)],
            commitment_hash,
        )
    }

    /// Constructs a checked, balanced 1-to-N double-entry transfer (one debit leg, N credit legs).
    /// Zero-amount credit legs are ignored; at least one positive credit leg is required.
    pub fn fan_out(
        entry_id: u64,
        category: impl Into<String>,
        from_account: impl Into<String>,
        credits: &[(String, u64)],
        commitment_hash: [u8; 32],
    ) -> Result<Self, String> {
        validate_non_zero_digest(&commitment_hash, "commitment_hash")?;

        let mut total: u64 = 0;
        for (_, amt) in credits {
            total = total
                .checked_add(*amt)
                .ok_or_else(|| "Journal total amount overflow".to_string())?;
        }
        if total == 0 {
            return Err("Journal entry must have non-zero movement".into());
        }
        let total_i64 = i64::try_from(total)
            .map_err(|_| "Journal total amount exceeds i64::MAX".to_string())?;

        let mut lines = Vec::with_capacity(credits.len() + 1);
        lines.push(JournalLine {
            account: from_account.into(),
            delta: -total_i64,
        });
        for (account, amt) in credits {
            if *amt == 0 {
                continue;
            }
            let delta = i64::try_from(*amt)
                .map_err(|_| "Journal credit leg exceeds i64::MAX".to_string())?;
            lines.push(JournalLine {
                account: account.clone(),
                delta,
            });
        }

        let entry = Self {
            entry_id,
            category: category.into(),
            lines,
            commitment_hash,
        };
        entry.validate_balanced()?;
        Ok(entry)
    }

    /// Enforces double-entry conservation: non-empty lines, non-zero legs, and `sum(delta) == 0`.
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
                "Conservation violation: double-entry journal sum is {sum} (must be 0)"
            ));
        }
        Ok(())
    }

    /// Computes a deterministic receipt leaf hash aligned with `muabase_db::Merkleizer::hash_entry`.
    pub fn receipt_leaf_hash(&self, key_prefix: &str) -> Result<[u8; 32], String> {
        self.validate_balanced()?;
        let key = format!("{key_prefix}/{}", self.entry_id);
        let value = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(dwal_merkle_leaf_hash(key.as_bytes(), &value))
    }
}
