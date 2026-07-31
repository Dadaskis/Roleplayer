//! Pure memory domain: the entity + input validation. No I/O (§5.2).

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum summary length; summaries are meant to be compact.
pub const MAX_SUMMARY_LENGTH: usize = 4000;

/// A long-term fact/summary belonging to a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Backend-generated UUID (v4); clients never supply it (§5.4).
    pub id: String,
    /// Owning campaign; memories are re-injected only into that campaign's
    /// context.
    pub campaign_id: String,
    /// The condensed fact; re-injected into the prompt verbatim.
    pub summary: String,
    /// First turn covered (inclusive).
    pub source_from: i64,
    /// Last turn covered (inclusive).
    pub source_to: i64,
    /// RFC 3339 timestamp; memories list newest first.
    pub created_at: String,
}

/// Input for creating a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMemory {
    /// Owning campaign; existence is enforced by the FK downstream.
    pub campaign_id: String,
    /// The condensed fact; validated for non-blank and length here.
    pub summary: String,
    /// First turn covered (inclusive); must not exceed [`source_to`].
    pub source_from: i64,
    /// Last turn covered (inclusive); must not precede [`source_from`].
    pub source_to: i64,
}

impl NewMemory {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        // Summaries are re-injected into the prompt, so keep them bounded:
        // an empty or runaway summary would waste context tokens.
        if self.summary.trim().is_empty() {
            // A blank summary would inject noise into the prompt; reject.
            return Err(AppError::Domain(
                "memory summary must not be empty".to_string(),
            ));
        }
        if self.summary.len() > MAX_SUMMARY_LENGTH {
            // Past the compactness budget; reject instead of bloating context.
            return Err(AppError::Domain(format!(
                "memory summary is too long (max {MAX_SUMMARY_LENGTH} chars)"
            )));
        }
        // A backwards turn range would confuse the context builder that
        // slices turns [source_from, source_to] inclusive.
        if self.source_to < self.source_from {
            // The slice logic below assumes from <= to; guard it up front.
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
        // A forward range [1, 10]; tests break the range on purpose.
        NewMemory {
            campaign_id: "camp-1".to_string(),
            summary: summary.to_string(),
            source_from: 1,
            source_to: 10,
        }
    }

    #[test]
    fn accepts_valid_memory() {
        // A non-blank summary with a forward turn range passes the check.
        assert!(new_memory("The party befriended the innkeeper.")
            .validate()
            .is_ok());
    }

    #[test]
    fn rejects_empty_summary() {
        // A blank summary would inject noise into the prompt; reject.
        assert!(new_memory("").validate().is_err());
    }

    #[test]
    fn rejects_inverted_turn_range() {
        // Flip the range so to < from; the validator must catch it.
        let mut memory = new_memory("summary");
        memory.source_from = 20;
        memory.source_to = 10;
        assert!(memory.validate().is_err());
    }
}
