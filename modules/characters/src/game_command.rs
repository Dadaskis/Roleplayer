//! The `create_character` game command: lets the GM populate the roster.
//!
//! This is the GM's tool for building the cast — NPCs and the player persona
//! alike. Like every command it is pure: it validates the model's arguments
//! and *declares* a [`StateMutation`]; the turn flow applies it to the
//! characters table (the command never touches storage itself, §4.6 of PLAN.md
//! and §5.3 of AGENTS.md).

use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::game_command::{
    CommandContext, GameCommand, StateMutation,
};
use roleplayer_core::llm::ToolSchema;
use serde_json::{json, Value};

/// The `create_character` tool. Registered with the turn flow alongside the
/// dice and world-update commands (§8 of AGENTS.md: implement + register).
pub struct CreateCharacterCommand;

impl GameCommand for CreateCharacterCommand {
    fn id(&self) -> &str {
        "create_character"
    }

    fn description(&self) -> &str {
        "Create a character in this campaign — the player persona or an NPC — with a name, a short bio, and optional stats."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "create_character".to_string(),
            description: self.description().to_string(),
            // JSON Schema object; only `name` is required. `stats` is an
            // untyped object so the model can record whatever a ruleset needs
            // (JSON first, §5.4 of AGENTS.md).
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The character's display name",
                    },
                    "bio": {
                        "type": "string",
                        "description": "A short backstory or description",
                    },
                    "is_player": {
                        "type": "boolean",
                        "description": "True when this is the player's persona; false (default) for an NPC",
                    },
                    "stats": {
                        "type": "object",
                        "description": "Attributes, HP, gold, or any other numbers",
                    }
                },
                "required": ["name"],
            }),
        }
    }

    fn execute(
        &self,
        arguments: Value,
        context: &mut CommandContext,
    ) -> Result<Value> {
        // The model's arguments are untrusted; require a real name. An empty
        // name would create an invisible roster row, so reject it up front.
        let name = arguments
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                AppError::Domain(
                    "create_character requires a non-empty 'name'".to_string(),
                )
            })?;
        // Optional fields degrade to safe defaults instead of erroring; a bio
        // may be absent and `stats` defaults to an empty object.
        let bio =
            arguments.get("bio").and_then(Value::as_str).unwrap_or_default();
        // `as_bool()` returns None for anything but a real boolean, so a model
        // that sends "yes" degrades to the NPC default rather than crashing.
        let is_player = arguments
            .get("is_player")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // `stats` is JSON by construction; an absent value becomes {}.
        let stats =
            arguments.get("stats").cloned().unwrap_or_else(|| json!({}));

        // Declare the creation; the turn flow routes this to the characters
        // service and returns the new row's details to the model.
        context.mutations.push(StateMutation::CreateCharacter {
            name: name.to_string(),
            bio: bio.to_string(),
            is_player,
            stats,
        });
        Ok(json!({
            "ok": true,
            "created": name,
            "is_player": is_player,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_a_create_character_mutation() {
        // A valid call must push the mutation (no storage, just the journal).
        let mut context = CommandContext {
            campaign_id: "c1".to_string(),
            world: json!({}),
            mutations: Vec::new(),
        };
        let result = CreateCharacterCommand
            .execute(
                json!({ "name": "Elara", "bio": "A ranger.", "is_player": true, "stats": { "hp": 12 } }),
                &mut context,
            )
            .expect("valid args");
        // The model sees a success summary with the created name.
        assert_eq!(result["created"], "Elara");
        // Exactly one mutation, carrying the generic fields for the turn flow.
        assert_eq!(context.mutations.len(), 1);
        match &context.mutations[0] {
            StateMutation::CreateCharacter { name, bio, is_player, stats } => {
                assert_eq!(name, "Elara");
                assert_eq!(bio, "A ranger.");
                assert!(*is_player);
                assert_eq!(stats["hp"], 12);
            }
            other => panic!("expected a character mutation, got {other:?}"),
        }
    }

    #[test]
    fn rejects_an_empty_name() {
        // The model must not be able to create a nameless roster row.
        let mut context = CommandContext {
            campaign_id: "c1".to_string(),
            world: json!({}),
            mutations: Vec::new(),
        };
        let result = CreateCharacterCommand
            .execute(json!({ "name": "  " }), &mut context);
        assert!(result.is_err());
        // Nothing was declared; the journal stays empty.
        assert!(context.mutations.is_empty());
    }
}
