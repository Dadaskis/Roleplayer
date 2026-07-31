//! Pure campaign domain: the entity and its validation rules.
//!
//! No I/O, no SQL, no Tauri — this module is only types + rules so it can be
//! unit-tested without infrastructure (§5.2 of AGENTS.md).

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum length of a campaign name; names are kept short for sidebars/UI.
pub const MAX_NAME_LENGTH: usize = 120;

/// A roleplay session. Owns everything below it in the aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id: String,
    pub name: String,
    pub description: String,
    pub ruleset_id: Option<String>,
    /// Free-form JSON settings (per-campaign model hint, flags, ...).
    pub settings: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCampaign {
    pub name: String,
    pub description: String,
    pub ruleset_id: Option<String>,
}

/// Input for updating a campaign (full replace of the editable fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCampaign {
    pub name: String,
    pub description: String,
    pub ruleset_id: Option<String>,
}

impl NewCampaign {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)
    }
}

impl UpdateCampaign {
    /// Validate update input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)
    }
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Domain(
            "campaign name must not be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_NAME_LENGTH {
        return Err(AppError::Domain(format!(
            "campaign name is too long (max {MAX_NAME_LENGTH} chars)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_campaign(name: &str) -> NewCampaign {
        NewCampaign {
            name: name.to_string(),
            description: String::new(),
            ruleset_id: None,
        }
    }

    #[test]
    fn accepts_valid_name() {
        assert!(new_campaign("The Wandering Merchant").validate().is_ok());
        // Whitespace around the name is tolerated and trimmed later.
        assert!(new_campaign("  Camp  ").validate().is_ok());
    }

    #[test]
    fn rejects_empty_name() {
        assert!(new_campaign("").validate().is_err());
        assert!(new_campaign("   ").validate().is_err());
    }

    #[test]
    fn rejects_overlong_name() {
        let long_name = "x".repeat(MAX_NAME_LENGTH + 1);
        assert!(new_campaign(&long_name).validate().is_err());
    }
}
