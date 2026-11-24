//! Pure commercial Muamalat escrow & 2-of-3 Tahkim arbitration state-transition rules.

use crate::tables::tables::{ContractTemplate, MuamalatContract, TahkimDispute};
use muabase_institutional::{
    validate_non_zero_digest, JournalEntry, ReviewerAttestation, TahkimPanel, TahkimRuling,
    TahkimVote,
};
use psibase::{AccountNumber, Checksum256};

pub const CONTRACT_KIND_ISTISNA: &str = "ISTISNA";
pub const CONTRACT_KIND_MUDARABAH: &str = "MUDARABAH";

pub const STATUS_CREATED: &str = "CREATED";
pub const STATUS_FUNDED: &str = "FUNDED";
pub const STATUS_DISPUTED: &str = "DISPUTED";
pub const STATUS_SETTLED: &str = "SETTLED";

pub const RULING_AWARD_CONTRACTOR: &str = "AWARD_CONTRACTOR";
pub const RULING_REFUND_FUNDER: &str = "REFUND_FUNDER";
pub const RULING_SPLIT: &str = "SPLIT";

pub const RECEIPT_PREFIX_MILESTONE: &str = "receipt/muamalat";
pub const RECEIPT_PREFIX_TAHKIM: &str = "receipt/tahkim";

/// Bounds milestones per contract so `contract_id * ENTRY_ID_STRIDE + (idx + 1)`
/// can never collide with `TAHKIM_AWARD_ENTRY_OFFSET` or the next contract's entry IDs.
pub const ENTRY_ID_STRIDE: u64 = 1000;
pub const TAHKIM_AWARD_ENTRY_OFFSET: u64 = 999;
pub const MAX_MILESTONES_PER_CONTRACT: usize = (TAHKIM_AWARD_ENTRY_OFFSET - 1) as usize;

pub fn validate_contract_kind(contract_kind: &str) {
    assert!(
        contract_kind == CONTRACT_KIND_ISTISNA || contract_kind == CONTRACT_KIND_MUDARABAH,
        "contract_kind must be {CONTRACT_KIND_ISTISNA} or {CONTRACT_KIND_MUDARABAH}"
    );
}

pub fn parse_ruling(
    ruling_kind: &str,
    funder_amount: u64,
    contractor_amount: u64,
) -> TahkimRuling {
    match ruling_kind {
        RULING_AWARD_CONTRACTOR => TahkimRuling::AwardCounterparty,
        RULING_REFUND_FUNDER => TahkimRuling::RefundFunder,
        RULING_SPLIT => TahkimRuling::Split {
            funder_amount,
            counterparty_amount: contractor_amount,
        },
        _ => panic!(
            "ruling_kind must be {RULING_AWARD_CONTRACTOR}, {RULING_REFUND_FUNDER}, or {RULING_SPLIT}"
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_validated_contract(
    contract_id: u64,
    tpl: &ContractTemplate,
    expected_template_hash: &Checksum256,
    funder: AccountNumber,
    contractor: AccountNumber,
    arbitrators: Vec<AccountNumber>,
    milestone_amounts: Vec<u64>,
    terms_commitment_hash: Checksum256,
) -> MuamalatContract {
    let att = ReviewerAttestation {
        reviewer_account: tpl.scholar_account.to_string(),
        template_hash: tpl.template_hash.0,
        certificate_hash: tpl.certificate_hash.0,
        standard_ref: tpl.standard_ref.clone(),
        revoked: !tpl.active,
    };
    att.validate_active(&expected_template_hash.0)
        .expect("Scholar template attestation verification failed");

    assert!(funder != contractor, "Funder and contractor must be distinct");
    validate_non_zero_digest(&terms_commitment_hash.0, "terms_commitment_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        arbitrators.len() == 3,
        "Tahkim panel requires exactly 3 distinct arbitrators"
    );
    let _panel = TahkimPanel::new([
        arbitrators[0].to_string(),
        arbitrators[1].to_string(),
        arbitrators[2].to_string(),
    ])
    .expect("Invalid tahkim arbitrator panel");
    assert!(
        !arbitrators.contains(&funder) && !arbitrators.contains(&contractor),
        "Arbitrators must be independent of funder and contractor"
    );
    assert!(
        !milestone_amounts.is_empty() && milestone_amounts.iter().all(|&m| m > 0),
        "All milestone_amounts must be strictly positive"
    );
    assert!(
        milestone_amounts.len() <= MAX_MILESTONES_PER_CONTRACT,
        "milestone_amounts cannot exceed {MAX_MILESTONES_PER_CONTRACT} entries per contract"
    );
    let total_escrow = milestone_amounts
        .iter()
        .try_fold(0u64, |acc, &m| acc.checked_add(m))
        .expect("total_escrow overflow");

    MuamalatContract {
        contract_id,
        template_id: tpl.template_id,
        template_hash: tpl.template_hash.clone(),
        contract_kind: tpl.contract_kind.clone(),
        funder,
        contractor,
        arbitrators,
        total_escrow,
        released_amount: 0,
        refunded_amount: 0,
        milestone_amounts,
        milestones_released: 0,
        terms_commitment_hash,
        status: STATUS_CREATED.to_string(),
        last_receipt_leaf_hash: None,
    }
}

pub fn apply_milestone_release(
    contract: &mut MuamalatContract,
    sender: AccountNumber,
    milestone_evidence_hash: &Checksum256,
) -> Checksum256 {
    assert_eq!(sender, contract.funder, "Only funder may release a milestone");
    assert_eq!(
        contract.status, STATUS_FUNDED,
        "Milestone release blocked: contract status is '{}' (must be {STATUS_FUNDED})",
        contract.status
    );
    validate_non_zero_digest(&milestone_evidence_hash.0, "milestone_evidence_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    let idx = contract.milestones_released as usize;
    assert!(
        idx < contract.milestone_amounts.len(),
        "All milestones already released"
    );
    let amount = contract.milestone_amounts[idx];

    let entry_id = contract
        .contract_id
        .checked_mul(ENTRY_ID_STRIDE)
        .and_then(|base| base.checked_add(idx as u64 + 1))
        .expect("milestone entry_id overflow");
    let entry = JournalEntry::transfer(
        entry_id,
        format!("{}:milestone_{}", contract.contract_kind, idx + 1),
        format!("escrow:{}", contract.contract_id),
        format!("contractor:{}", contract.contractor),
        amount,
        milestone_evidence_hash.0,
    )
    .expect("Milestone double-entry conservation violated");

    let leaf = Checksum256::from(
        entry
            .receipt_leaf_hash(RECEIPT_PREFIX_MILESTONE)
            .expect("Milestone receipt digest failed"),
    );

    contract.released_amount = contract
        .released_amount
        .checked_add(amount)
        .expect("released_amount overflow");
    contract.milestones_released += 1;
    contract.last_receipt_leaf_hash = Some(leaf.clone());
    if (contract.milestones_released as usize) == contract.milestone_amounts.len() {
        contract.status = STATUS_SETTLED.to_string();
    }
    leaf
}

pub fn apply_raise_dispute(
    contract: &mut MuamalatContract,
    sender: AccountNumber,
    evidence_commitment_hash: Checksum256,
) -> TahkimDispute {
    assert!(
        sender == contract.funder || sender == contract.contractor,
        "Only funder or contractor may raise a dispute"
    );
    assert_eq!(
        contract.status, STATUS_FUNDED,
        "Disputes may only be raised on a {STATUS_FUNDED} contract"
    );
    validate_non_zero_digest(&evidence_commitment_hash.0, "evidence_commitment_hash")
        .unwrap_or_else(|e| panic!("{e}"));
    contract.status = STATUS_DISPUTED.to_string();
    TahkimDispute {
        contract_id: contract.contract_id,
        raised_by: sender,
        evidence_commitment_hash,
        votes_json: vec![],
        resolved: false,
        final_ruling: None,
        award_receipt_leaf_hash: None,
    }
}

pub fn apply_tahkim_vote(
    contract: &mut MuamalatContract,
    dispute: &mut TahkimDispute,
    arbitrator: AccountNumber,
    ruling: TahkimRuling,
) -> Option<Checksum256> {
    assert_eq!(
        contract.status, STATUS_DISPUTED,
        "Contract is not in {STATUS_DISPUTED} state"
    );
    assert!(!dispute.resolved, "Dispute is already resolved");

    let remaining = contract
        .total_escrow
        .checked_sub(contract.released_amount)
        .expect("released_amount exceeds total_escrow");

    let mut panel = TahkimPanel::new([
        contract.arbitrators[0].to_string(),
        contract.arbitrators[1].to_string(),
        contract.arbitrators[2].to_string(),
    ])
    .expect("Corrupt panel");
    for raw in &dispute.votes_json {
        let v: TahkimVote = serde_json::from_str(raw).expect("Corrupt stored TahkimVote");
        panel.votes.push(v);
    }

    let resolved = panel
        .cast_vote(&arbitrator.to_string(), ruling, remaining)
        .expect("Tahkim vote rejected");

    dispute.votes_json = panel
        .votes
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect();

    if let Some(final_ruling) = resolved {
        let (funder_back, contractor_pay) = match final_ruling {
            TahkimRuling::AwardCounterparty => (0, remaining),
            TahkimRuling::RefundFunder => (remaining, 0),
            TahkimRuling::Split {
                funder_amount,
                counterparty_amount,
            } => (funder_amount, counterparty_amount),
        };
        assert_eq!(
            funder_back.checked_add(contractor_pay),
            Some(remaining),
            "Tahkim award must conserve remaining escrow"
        );

        let entry_id = contract
            .contract_id
            .checked_mul(ENTRY_ID_STRIDE)
            .and_then(|base| base.checked_add(TAHKIM_AWARD_ENTRY_OFFSET))
            .expect("tahkim award entry_id overflow");
        let entry = JournalEntry::fan_out(
            entry_id,
            "tahkim_award",
            format!("escrow:{}", contract.contract_id),
            &[
                (format!("funder:{}", contract.funder), funder_back),
                (format!("contractor:{}", contract.contractor), contractor_pay),
            ],
            dispute.evidence_commitment_hash.0,
        )
        .expect("Tahkim award conservation violated");

        let leaf = Checksum256::from(
            entry
                .receipt_leaf_hash(RECEIPT_PREFIX_TAHKIM)
                .expect("Tahkim award receipt digest failed"),
        );

        contract.released_amount = contract
            .released_amount
            .checked_add(contractor_pay)
            .expect("released_amount overflow");
        contract.refunded_amount = contract
            .refunded_amount
            .checked_add(funder_back)
            .expect("refunded_amount overflow");
        contract.status = STATUS_SETTLED.to_string();
        contract.last_receipt_leaf_hash = Some(leaf.clone());
        dispute.resolved = true;
        dispute.final_ruling = Some(serde_json::to_string(&final_ruling).unwrap());
        dispute.award_receipt_leaf_hash = Some(leaf.clone());
        Some(leaf)
    } else {
        None
    }
}
