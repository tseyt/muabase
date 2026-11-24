//! Pluggable 2-of-3 Tahkim arbitration panel with exact escrow conservation.

use serde::{Deserialize, Serialize};

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

    /// Returns the winning ruling if at least 2 arbitrators have already agreed.
    pub fn resolved_ruling(&self) -> Option<TahkimRuling> {
        for vote in &self.votes {
            let count = self.votes.iter().filter(|v| v.ruling == vote.ruling).count();
            if count >= 2 {
                return Some(vote.ruling.clone());
            }
        }
        None
    }

    pub fn cast_vote(
        &mut self,
        arbitrator: &str,
        ruling: TahkimRuling,
        total_escrow: u64,
    ) -> Result<Option<TahkimRuling>, String> {
        if self.resolved_ruling().is_some() {
            return Err("Tahkim panel has already reached a 2-of-3 quorum".into());
        }
        if !self.arbitrators.iter().any(|a| a == arbitrator) {
            return Err(format!(
                "'{arbitrator}' is not on the tahkim arbitrator panel"
            ));
        }
        if self.votes.iter().any(|v| v.arbitrator == arbitrator) {
            return Err(format!("Arbitrator '{arbitrator}' has already voted"));
        }
        if let TahkimRuling::Split {
            funder_amount,
            counterparty_amount,
        } = ruling
        {
            if funder_amount.checked_add(counterparty_amount) != Some(total_escrow) {
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
