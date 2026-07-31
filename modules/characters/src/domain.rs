//! Pure character domain: the entity and validation rules. No I/O (§5.2).

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum length of a character name.
pub const MAX_NAME_LENGTH: usize = 80;

/// A character (player or NPC) belonging to a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    /// Backend-generated UUID (v4); clients never supply it (§5.4).
    pub id: String,
    /// Owning campaign; enforces the aggregate boundary (§5.4).
    pub campaign_id: String,
    /// Display name; validated by [`validate_name`], then trimmed.
    pub name: String,
    /// Players surface first in lists and are the user's own persona.
    pub is_player: bool,
    /// Free-text backstory shown in the character sheet.
    pub bio: String,
    /// Free-form stats (attributes, HP, gold, ...) — JSON first (§5.4).
    pub stats: serde_json::Value,
    /// Free-form extra data for rulesets/modules to hang onto.
    pub extra: serde_json::Value,
    /// RFC 3339 timestamp, set once at insert.
    pub created_at: String,
}

/// Input for creating a character.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCharacter {
    /// Owning campaign; validated as non-blank here, existence via FK.
    pub campaign_id: String,
    /// Display name; the only other field validated at this boundary.
    pub name: String,
    /// Players surface first in lists and are the user's own persona.
    pub is_player: bool,
    /// Free-text backstory.
    pub bio: String,
    /// Free-form stats; shape is intentionally unconstrained (§5.4).
    pub stats: serde_json::Value,
}

/// Input for updating a character (full replace of editable fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCharacter {
    /// New display name; replaces the stored one wholesale.
    pub name: String,
    /// New player/NPC flag; replaces the stored one wholesale.
    pub is_player: bool,
    /// New backstory; replaces the stored one wholesale.
    pub bio: String,
    /// New stats; replaces the stored one wholesale.
    pub stats: serde_json::Value,
}

impl NewCharacter {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        // A character must reference a campaign; reject blanks early with a
        // domain error instead of a raw DB constraint failure.
        if self.campaign_id.trim().is_empty() {
            // An empty owner would orphan the row in the aggregate tree.
            return Err(AppError::Domain(
                "character must belong to a campaign".to_string(),
            ));
        }
        Ok(())
    }
}

impl UpdateCharacter {
    /// Validate update input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        // Updates cannot reparent a character (no campaign_id field), so the
        // name rule is the only check needed here.
        validate_name(&self.name)
    }
}

fn validate_name(name: &str) -> Result<()> {
    // Validate the trimmed form, so whitespace-only names fail the check.
    let trimmed = name.trim();
    if trimmed.is_empty() {
        // A blank name would render as an unlabelled sheet; reject early.
        return Err(AppError::Domain(
            "character name must not be empty".to_string(),
        ));
    }
    // Bound the trimmed form too, so padding cannot smuggle an overlong name.
    if trimmed.len() > MAX_NAME_LENGTH {
        // Past the sheet display limit; reject instead of truncating data.
        return Err(AppError::Domain(format!(
            "character name is too long (max {MAX_NAME_LENGTH} chars)"
        )));
    }
    // Both checks passed; the caller may proceed toward storage.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_character(name: &str) -> NewCharacter {
        // Minimal input; each test varies exactly one field to isolate it.
        NewCharacter {
            campaign_id: "camp-1".to_string(),
            name: name.to_string(),
            is_player: true,
            bio: String::new(),
            stats: serde_json::json!({}),
        }
    }

    #[test]
    fn accepts_valid_character() {
        // A normal name with a real campaign passes the domain check.
        assert!(new_character("Elara").validate().is_ok());
    }

    #[test]
    fn rejects_empty_or_blank_name() {
        // Both the bare empty string and an all-whitespace one must fail.
        assert!(new_character("").validate().is_err());
        assert!(new_character("   ").validate().is_err());
    }

    #[test]
    fn rejects_overlong_name() {
        // Exactly one character past the cap trips the length check.
        let long_name = "x".repeat(MAX_NAME_LENGTH + 1);
        assert!(new_character(&long_name).validate().is_err());
    }

    #[test]
    fn rejects_missing_campaign() {
        // A whitespace-only owner id is rejected before it reaches the FK.
        let mut input = new_character("Elara");
        input.campaign_id = " ".to_string();
        assert!(input.validate().is_err());
    }
}
