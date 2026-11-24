//! Scholar/auditor reviewer attestation and deterministic disbursement compliance policies.

use crate::validation::validate_non_zero_digest;
use serde::{Deserialize, Serialize};

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
        if self.standard_ref.trim().is_empty() {
            return Err("Attestation standard_ref must be non-empty".into());
        }
        validate_non_zero_digest(&self.template_hash, "template_hash")?;
        validate_non_zero_digest(&self.certificate_hash, "certificate_hash")?;
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
                "Policy cap exceeded: {amount} > {}",
                self.max_single_disbursement
            ));
        }
        if !self.allowed_categories.is_empty()
            && !self.allowed_categories.iter().any(|c| c == category)
        {
            return Err(format!(
                "Shurut violation: category '{category}' is not allowed"
            ));
        }
        if !self.allowlist_recipients.is_empty()
            && !self.allowlist_recipients.iter().any(|r| r == recipient)
        {
            return Err(format!(
                "Recipient '{recipient}' is not on policy allowlist"
            ));
        }
        Ok(())
    }
}
