//! Pure Waqf corpus-preservation, shurut compliance, and board-quorum transition rules.

use crate::tables::tables::{DisbursementProposal, Endowment, KeyRecoveryProposal};
use muabase_institutional::{validate_non_zero_digest, CompliancePolicy, JournalEntry};
use psibase::{AccountNumber, Checksum256};

pub const RECEIPT_KEY_PREFIX: &str = "receipt/waqf";

pub fn validate_endowment_params(
    label: &str,
    corpus_principal: u64,
    shurut_hash: &Checksum256,
    scholar_attestation_hash: &Checksum256,
    allowed_categories: &[String],
    board_members: &[AccountNumber],
    quorum_threshold: u32,
) {
    assert!(!label.trim().is_empty(), "Endowment label must be non-empty");
    assert!(
        corpus_principal > 0,
        "Endowment corpus_principal must be positive"
    );
    validate_non_zero_digest(&shurut_hash.0, "shurut_hash").unwrap_or_else(|e| panic!("{e}"));
    validate_non_zero_digest(&scholar_attestation_hash.0, "scholar_attestation_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        !allowed_categories.is_empty() && allowed_categories.iter().all(|c| !c.trim().is_empty()),
        "allowed_categories (shurut) must contain at least one non-empty category"
    );
    assert!(
        quorum_threshold >= 2 && (quorum_threshold as usize) <= board_members.len(),
        "Board quorum_threshold must be >= 2 and <= board_members.len()"
    );
    for (i, a) in board_members.iter().enumerate() {
        for b in &board_members[i + 1..] {
            assert!(a != b, "Duplicate board member in Waqf board");
        }
    }
}

pub fn validate_disbursement_rules(
    endowment: &Endowment,
    recipient: AccountNumber,
    category: &str,
    amount: u64,
    evidence_commitment_hash: &Checksum256,
) {
    assert!(endowment.principal_locked, "Waqf corpus lock must be active");
    validate_non_zero_digest(&evidence_commitment_hash.0, "evidence_commitment_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        amount <= endowment.yield_balance,
        "Waqf Corpus Lock Violation: disbursement ({}) exceeds available yield ({}); principal ({}) cannot be spent",
        amount,
        endowment.yield_balance,
        endowment.corpus_principal
    );

    let policy = CompliancePolicy {
        max_single_disbursement: endowment.max_single_disbursement,
        allowed_categories: endowment.allowed_categories.clone(),
        allowlist_recipients: vec![],
        require_zero_fee_tamlik: true,
    };
    policy
        .validate_disbursement(&recipient.to_string(), category, amount, 0)
        .expect("Shurut policy check failed");
}

pub fn validate_key_recovery_proposal(
    endowment: &Endowment,
    proposer: AccountNumber,
    old_member: AccountNumber,
    new_member: AccountNumber,
    new_pubkey_spki_hash: &Checksum256,
) {
    assert!(
        endowment.board_members.contains(&proposer) && proposer != old_member,
        "Key recovery must be proposed by another active board member"
    );
    assert!(
        endowment.board_members.contains(&old_member),
        "old_member is not on the endowment board"
    );
    assert!(
        !endowment.board_members.contains(&new_member),
        "new_member is already on the endowment board"
    );
    validate_non_zero_digest(&new_pubkey_spki_hash.0, "new_pubkey_spki_hash")
        .unwrap_or_else(|e| panic!("{e}"));
}

/// Records an active board member's approval and returns whether active-member quorum is met.
/// Approvals from previously rotated-out members do not count toward quorum.
fn record_board_approval(
    endowment: &Endowment,
    approvals: &mut Vec<AccountNumber>,
    approver: AccountNumber,
    excluded_member: Option<AccountNumber>,
) -> bool {
    let is_excluded = excluded_member == Some(approver);
    assert!(
        endowment.board_members.contains(&approver) && !is_excluded,
        "Only an eligible active board member may approve this proposal"
    );
    assert!(
        !approvals.contains(&approver),
        "Board member has already approved this proposal"
    );
    approvals.push(approver);
    let active_approvals = approvals
        .iter()
        .filter(|m| endowment.board_members.contains(m) && excluded_member != Some(**m))
        .count();
    (active_approvals as u32) >= endowment.quorum_threshold
}

pub fn settle_disbursement(
    endowment: &mut Endowment,
    prop: &mut DisbursementProposal,
) -> Checksum256 {
    validate_disbursement_rules(
        endowment,
        prop.recipient,
        &prop.category,
        prop.amount,
        &prop.evidence_commitment_hash,
    );

    let entry = JournalEntry::transfer(
        prop.proposal_id,
        prop.category.clone(),
        format!("waqf:{}:yield", endowment.endowment_id),
        format!("recipient:{}", prop.recipient),
        prop.amount,
        prop.evidence_commitment_hash.0,
    )
    .expect("Double-entry Waqf conservation violated");

    let leaf = Checksum256::from(
        entry
            .receipt_leaf_hash(RECEIPT_KEY_PREFIX)
            .expect("Double-entry Waqf receipt digest failed"),
    );

    endowment.yield_balance -= prop.amount;
    endowment.total_disbursed = endowment
        .total_disbursed
        .checked_add(prop.amount)
        .expect("total_disbursed overflow");
    prop.executed = true;
    prop.receipt_leaf_hash = Some(leaf.clone());
    leaf
}

pub fn apply_disbursement_approval(
    endowment: &mut Endowment,
    prop: &mut DisbursementProposal,
    approver: AccountNumber,
) -> Option<Checksum256> {
    assert!(!prop.executed, "DisbursementProposal already executed");
    if record_board_approval(endowment, &mut prop.approvals, approver, None) {
        Some(settle_disbursement(endowment, prop))
    } else {
        None
    }
}

pub fn apply_key_recovery_approval(
    endowment: &mut Endowment,
    rec: &mut KeyRecoveryProposal,
    approver: AccountNumber,
) -> bool {
    assert!(!rec.executed, "KeyRecoveryProposal already executed");
    if record_board_approval(endowment, &mut rec.approvals, approver, Some(rec.old_member)) {
        assert!(
            !endowment.board_members.contains(&rec.new_member),
            "new_member is already on the endowment board"
        );
        let pos = endowment
            .board_members
            .iter()
            .position(|m| *m == rec.old_member)
            .expect("old_member no longer on board");
        endowment.board_members[pos] = rec.new_member;
        rec.executed = true;
        true
    } else {
        false
    }
}
