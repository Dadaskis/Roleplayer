//! Pure character domain: the entity and validation rules. No I/O (§5.2).

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum length of a character name.
pub const MAX_NAME_LENGTH: usize = 80;

/// A character (player or NPC) belonging to a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub campaign_id: String,
    pub name: String,
    pub is_player: bool,
    pub bio: String,
    /// Free-form stats (attributes, HP, gold, ...) — JSON first (§5.4).
    pub stats: serde_json::Value,
    /// Free-form extra data for rulesets/modules to hang onto.
    pub extra: serde_json::Value,
    pub created_at: String,
}

/// Input for creating a character.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCharacter {
    pub campaign_id: String,
    pub name: String,
    pub is_player: bool,
    pub bio: String,
    pub stats: serde_json::Value,
}

/// Input for updating a character (full replace of editable fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCharacter {
    pub name: String,
    pub is_player: bool,
    pub bio: String,
    pub stats: serde_json::Value,
}

impl NewCharacter {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        if self.campaign_id.trim().is_empty() {
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
        validate_name(&self.name)
    }
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Domain(
            "character name must not be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_NAME_LENGTH {
        return Err(AppError::Domain(format!(
            "character name is too long (max {MAX_NAME_LENGTH} chars)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_character(name: &str) -> NewCharacter {
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
        assert!(new_character("Elara").validate().is_ok());
    }

    #[test]
    fn rejects_empty_or_blank_name() {
        assert!(new_character("").validate().is_err());
        assert!(new_character("   ").validate().is_err());
    }

    #[test]
    fn rejects_overlong_name() {
        let long_name = "x".repeat(MAX_NAME_LENGTH + 1);
        assert!(new_character(&long_name).validate().is_err());
    }

    #[test]
    fn rejects_missing_campaign() {
        let mut input = new_character("Elara");
        input.campaign_id = " ".to_string();
        assert!(input.validate().is_err());
    }
}
