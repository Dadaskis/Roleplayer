//! Pure campaign domain: the entity and its validation rules.
//!
//! No I/O, no SQL, no Tauri — this module is only types + rules so it can be
//! unit-tested without infrastructure (§5.2 of AGENTS.md).

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum length of a campaign name; names are kept short for sidebars/UI.
pub const MAX_NAME_LENGTH: usize = 120;

/// The lifecycle state of a campaign (a closed state machine, §5.4).
///
/// The GM asks clarifying questions while `Setup`, generates the world and
/// characters during the transient `Worldgen` turn, then narrates normally
/// once `Active`. The status is backend-driven only — never client-editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    /// The GM is asking questions before the world exists.
    Setup,
    /// A world-generation turn is in flight (single-flight guard).
    Worldgen,
    /// Normal play: the world exists and the story runs.
    Active,
}

impl CampaignStatus {
    /// Stable wire name used in persistence and IPC.
    pub fn as_str(&self) -> &'static str {
        match self {
            CampaignStatus::Setup => "setup",
            CampaignStatus::Worldgen => "worldgen",
            CampaignStatus::Active => "active",
        }
    }

    /// Inverse of [`CampaignStatus::as_str`]; unknown strings fall back to
    /// `Setup` so pre-v3 rows and malformed values degrade gracefully.
    pub fn from_wire(value: &str) -> CampaignStatus {
        match value {
            "worldgen" => CampaignStatus::Worldgen,
            "active" => CampaignStatus::Active,
            _ => CampaignStatus::Setup,
        }
    }
}

/// A roleplay session. Owns everything below it in the aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    /// Backend-generated UUID (v4); clients never supply it (§5.4).
    pub id: String,
    /// Short display name; validated by [`validate_name`], then trimmed.
    pub name: String,
    /// Free-text summary shown on the campaign list screen.
    pub description: String,
    /// Optional GM "brain" preset; None falls back to default behaviour.
    pub ruleset_id: Option<String>,
    /// Which lifecycle phase the campaign is in (drives the GM prompt).
    pub status: CampaignStatus,
    /// Free-form JSON settings (per-campaign model hint, flags, ...).
    pub settings: serde_json::Value,
    /// RFC 3339 timestamp, set once at insert; drives newest-first lists.
    pub created_at: String,
    /// RFC 3339 timestamp, refreshed by every update.
    pub updated_at: String,
}

/// Input for creating a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCampaign {
    /// Display name; the only field validated at this boundary.
    pub name: String,
    /// Optional free-text summary.
    pub description: String,
    /// Optional ruleset preset; None means default GM behaviour.
    pub ruleset_id: Option<String>,
}

/// Input for updating a campaign (full replace of the editable fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCampaign {
    /// New display name; replaces the stored one wholesale.
    pub name: String,
    /// New summary; replaces the stored one wholesale.
    pub description: String,
    /// New ruleset link; None clears it (full replace).
    pub ruleset_id: Option<String>,
}

impl NewCampaign {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        // Name is the only hard constraint at create time; description is
        // free-form and ruleset ownership is checked by the service.
        validate_name(&self.name)
    }
}

impl UpdateCampaign {
    /// Validate update input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        // Same rule set as create; a rename must stay within the limits.
        validate_name(&self.name)
    }
}

fn validate_name(name: &str) -> Result<()> {
    // Validate the trimmed form, so whitespace-only names fail the check.
    let trimmed = name.trim();
    if trimmed.is_empty() {
        // A blank name would render as an unlabelled row; reject early.
        return Err(AppError::Domain(
            "campaign name must not be empty".to_string(),
        ));
    }
    // Bound the trimmed form too, so padding cannot smuggle an overlong name.
    if trimmed.len() > MAX_NAME_LENGTH {
        // Past the sidebar display limit; reject instead of truncating data.
        return Err(AppError::Domain(format!(
            "campaign name is too long (max {MAX_NAME_LENGTH} chars)"
        )));
    }
    // Both checks passed; the caller may proceed toward storage.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_campaign(name: &str) -> NewCampaign {
        // Minimal input; each test varies exactly one field to isolate it.
        NewCampaign {
            name: name.to_string(),
            description: String::new(),
            ruleset_id: None,
        }
    }

    #[test]
    fn accepts_valid_name() {
        // A normal name passes the domain check.
        assert!(new_campaign("The Wandering Merchant").validate().is_ok());
        // Whitespace around the name is tolerated and trimmed later.
        assert!(new_campaign("  Camp  ").validate().is_ok());
    }

    #[test]
    fn rejects_empty_name() {
        // Both the bare empty string and an all-whitespace one must fail.
        assert!(new_campaign("").validate().is_err());
        assert!(new_campaign("   ").validate().is_err());
    }

    #[test]
    fn rejects_overlong_name() {
        // Exactly one character past the cap trips the length check.
        let long_name = "x".repeat(MAX_NAME_LENGTH + 1);
        assert!(new_campaign(&long_name).validate().is_err());
    }
}
