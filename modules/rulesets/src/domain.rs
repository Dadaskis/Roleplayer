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
    /// Backend-generated UUID (v4); clients never supply it (§5.4).
    pub id: String,
    /// Display name; validated by [`validate_name`], then trimmed.
    pub name: String,
    /// The behaviour-defining instruction text sent to the GM model.
    pub system_prompt: String,
    /// Free-form house rules document (JSON) merged into the system prompt.
    pub world_rules: serde_json::Value,
    /// Built-in presets are seeded and not deletable.
    pub is_builtin: bool,
    /// RFC 3339 timestamp, set once at insert; drives list ordering.
    pub created_at: String,
}

/// Input for creating a ruleset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRuleset {
    /// Display name; validated at this boundary.
    pub name: String,
    /// Behaviour text; must be non-blank (a GM needs instructions).
    pub system_prompt: String,
    /// Free-form house rules; merged into the prompt at turn time.
    pub world_rules: serde_json::Value,
}

/// Input for updating a ruleset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuleset {
    /// New display name; replaces the stored one wholesale.
    pub name: String,
    /// New behaviour text; replaces the stored one wholesale.
    pub system_prompt: String,
    /// New house rules; replaces the stored one wholesale.
    pub world_rules: serde_json::Value,
}

impl NewRuleset {
    /// Validate creation input before it reaches storage (§5.10).
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        // A blank system prompt would produce a directionless GM; reject it
        // before storage rather than seeding a useless preset.
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
        // Same constraint as create: editing must not blank the prompt.
        if self.system_prompt.trim().is_empty() {
            return Err(AppError::Domain(
                "system prompt must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    // Validate the trimmed form, so whitespace-only names fail the check.
    let trimmed = name.trim();
    if trimmed.is_empty() {
        // A blank name would render as an unlabelled preset; reject early.
        return Err(AppError::Domain(
            "ruleset name must not be empty".to_string(),
        ));
    }
    // Bound the trimmed form too, so padding cannot smuggle an overlong name.
    if trimmed.len() > MAX_NAME_LENGTH {
        // Past the picker display limit; reject instead of truncating data.
        return Err(AppError::Domain(format!(
            "ruleset name is too long (max {MAX_NAME_LENGTH} chars)"
        )));
    }
    // Both checks passed; the caller may proceed toward storage.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_ruleset(name: &str, prompt: &str) -> NewRuleset {
        // Minimal input; each test varies exactly one field to isolate it.
        NewRuleset {
            name: name.to_string(),
            system_prompt: prompt.to_string(),
            world_rules: serde_json::json!({}),
        }
    }

    #[test]
    fn accepts_valid_ruleset() {
        // A name plus a non-blank prompt passes the domain check.
        assert!(new_ruleset("Grimdark", "You are the GM.").validate().is_ok());
    }

    #[test]
    fn rejects_empty_name_or_prompt() {
        // Both the missing name and the blank prompt must be rejected.
        assert!(new_ruleset("", "prompt").validate().is_err());
        assert!(new_ruleset("name", "  ").validate().is_err());
    }
}
