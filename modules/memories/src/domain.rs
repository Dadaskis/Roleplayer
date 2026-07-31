//! Pure memory domain: the entity + input validation. No I/O (§5.2).

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum summary length; summaries are meant to be compact.
pub const MAX_SUMMARY_LENGTH: usize = 4000;

/// A long-term fact/summary belonging to a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub campaign_id: String,
    pub summary: String,
    /// First turn covered (inclusive).
    pub source_from: i64,
    /// Last turn covered (inclusive).
    pub source_to: i64,
    pub created_at: String,
}

/// Input for creating a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMemory {
    pub campaign_id: String,
    pub summary: String,
    pub source_from: i64,
    pub source_to: i64,
}

impl NewMemory {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        if self.summary.trim().is_empty() {
            return Err(AppError::Domain(
                "memory summary must not be empty".to_string(),
            ));
        }
        if self.summary.len() > MAX_SUMMARY_LENGTH {
            return Err(AppError::Domain(format!(
                "memory summary is too long (max {MAX_SUMMARY_LENGTH} chars)"
            )));
        }
        if self.source_to < self.source_from {
            return Err(AppError::Domain(
                "memory turn range is inverted (to < from)".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_memory(summary: &str) -> NewMemory {
        NewMemory {
            campaign_id: "camp-1".to_string(),
            summary: summary.to_string(),
            source_from: 1,
            source_to: 10,
        }
    }

    #[test]
    fn accepts_valid_memory() {
        assert!(new_memory("The party befriended the innkeeper.")
            .validate()
            .is_ok());
    }

    #[test]
    fn rejects_empty_summary() {
        assert!(new_memory("").validate().is_err());
    }

    #[test]
    fn rejects_inverted_turn_range() {
        let mut memory = new_memory("summary");
        memory.source_from = 20;
        memory.source_to = 10;
        assert!(memory.validate().is_err());
    }
}
