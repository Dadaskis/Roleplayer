//! Pure ruleset domain: the entity, validation, and the built-in default.

use roleplayer_core::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Maximum length of a ruleset name.
pub const MAX_NAME_LENGTH: usize = 100;

/// Name of the built-in ruleset seeded on first run.
pub const DEFAULT_RULESET_NAME: &str = "Standard Fantasy GM";

/// The default GM system prompt — a focused, hallucination-aware instruction
/// set (§4.6 of PLAN.md): narrative only, world changes only via tools,
/// storage is the source of truth.
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are the Game Master of a text roleplay.

BEHAVIOUR
- Narrate vivid, coherent scenes in the present tense, second person ("you").
- Keep the story going: react to the player's actions, create consequences,
  and move the world forward. Do not end the scene unless it is a natural
  resting point.
- Respect the world rules and the current world state below. If the world
  state conflicts with something you "remember", the world state is the truth.

WORLD CHANGES (very important)
- You change the world ONLY by calling tools. Never claim in narration that
  something changed in the world unless you actually applied it with a tool.
- Free text in your replies is narrative only — it does not mutate state.
- When a fact should persist (a room is now on fire, an NPC is dead, the
  player gained an item), update it with the available tools, then narrate.
- Never invent tool results; only call a tool when you genuinely need to
  record a change or roll dice.

TURNS
- Address the player's intent directly. If an action is uncertain or opposed,
  use the dice tool to resolve it fairly.
- Keep replies focused: a few paragraphs per turn is plenty."#;

/// A reusable GM preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ruleset {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    /// Free-form house rules document (JSON) merged into the system prompt.
    pub world_rules: serde_json::Value,
    /// Built-in presets are seeded and not deletable.
    pub is_builtin: bool,
    pub created_at: String,
}

/// Input for creating a ruleset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRuleset {
    pub name: String,
    pub system_prompt: String,
    pub world_rules: serde_json::Value,
}

/// Input for updating a ruleset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuleset {
    pub name: String,
    pub system_prompt: String,
    pub world_rules: serde_json::Value,
}

impl NewRuleset {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        if self.system_prompt.trim().is_empty() {
            return Err(AppError::Domain(
                "system prompt must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

impl UpdateRuleset {
    /// Validate update input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        if self.system_prompt.trim().is_empty() {
            return Err(AppError::Domain(
                "system prompt must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Domain(
            "ruleset name must not be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_NAME_LENGTH {
        return Err(AppError::Domain(format!(
            "ruleset name is too long (max {MAX_NAME_LENGTH} chars)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_ruleset(name: &str, prompt: &str) -> NewRuleset {
        NewRuleset {
            name: name.to_string(),
            system_prompt: prompt.to_string(),
            world_rules: serde_json::json!({}),
        }
    }

    #[test]
    fn accepts_valid_ruleset() {
        assert!(new_ruleset("Grimdark", "You are the GM.").validate().is_ok());
    }

    #[test]
    fn rejects_empty_name_or_prompt() {
        assert!(new_ruleset("", "prompt").validate().is_err());
        assert!(new_ruleset("name", "  ").validate().is_err());
    }
}
