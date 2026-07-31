//! Pure turn domain: dice parsing/rolling and the v1 game commands.
//!
//! Commands implement [`GameCommand`] and only ever *declare* world mutations
//! (they push into the context journal); the turn flow applies them. This keeps
//! the command layer pure and the world under a single writer.

use rand::Rng;
use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::game_command::{
    CommandContext, GameCommand, StateMutation,
};
use roleplayer_core::llm::ToolSchema;
use serde_json::{json, Value};

/// Maximum number of dice in a single roll (bounds hostile input, §5.10).
const MAX_DICE_COUNT: u32 = 100;

/// Maximum face value (d2 .. d1000).
const MAX_DICE_SIDES: u32 = 1000;

/// Maximum absolute modifier.
const MAX_MODIFIER: i32 = 10_000;

/// A parsed dice expression like `2d6+1` or `d20-2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceSpec {
    pub count: u32,
    pub sides: u32,
    pub modifier: i32,
}

/// The result of a single roll.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiceRoll {
    pub expression: String,
    pub total: i32,
    pub rolls: Vec<u32>,
}

/// Parse a dice expression. Format: `[count]d<sides>[+|-modifier]`.
///
/// Manual parser (no regex): split on the first `d`, then split the trailing
/// part on the first `+` or `-`. Everything is validated against sane bounds so
/// a hostile expression (huge count, absurd sides) fails with a typed error.
pub fn parse_dice_expression(expression: &str) -> Result<DiceSpec> {
    let expression = expression.trim().to_lowercase();
    if expression.is_empty() {
        return Err(AppError::Domain(
            "dice expression must not be empty".to_string(),
        ));
    }

    let (dice_part, modifier) = split_modifier(&expression)?;
    let (count_part, sides_part) =
        dice_part.split_once('d').ok_or_else(|| {
            AppError::Domain(format!("not a dice expression: {expression}"))
        })?;

    let count = if count_part.is_empty() {
        1
    } else {
        parse_u32(count_part, "dice count")?
    };
    let sides = parse_u32(sides_part, "dice sides")?;

    if count == 0 || count > MAX_DICE_COUNT {
        return Err(AppError::Domain(format!(
            "dice count must be between 1 and {MAX_DICE_COUNT}"
        )));
    }
    if sides == 0 || sides > MAX_DICE_SIDES {
        return Err(AppError::Domain(format!(
            "dice sides must be between 1 and {MAX_DICE_SIDES}"
        )));
    }
    if modifier.abs() > MAX_MODIFIER {
        return Err(AppError::Domain(
            "dice modifier is out of range".to_string(),
        ));
    }

    Ok(DiceSpec { count, sides, modifier })
}

/// Split `2d6+1` into `("2d6", 1)`; handles `d20`, `2d6-2`, `2d6`.
fn split_modifier(expression: &str) -> Result<(&str, i32)> {
    // Locate the first '+' or '-' that appears after the 'd' so `-` in nothing
    // else can confuse the parse (dice parts never contain signs otherwise).
    let dice_index = expression.find('d').ok_or_else(|| {
        AppError::Domain(format!("not a dice expression: {expression}"))
    })?;
    let sign_index = expression[dice_index + 1..]
        .find(['+', '-'])
        .map(|offset| dice_index + 1 + offset);

    match sign_index {
        Some(index) => {
            let dice_part = &expression[..index];
            let sign = expression[index..].chars().next();
            let magnitude: i32 =
                expression[index + 1..].trim().parse().map_err(|_| {
                    AppError::Domain(
                        "dice modifier must be an integer".to_string(),
                    )
                })?;
            let modifier = match sign {
                Some('-') => -magnitude,
                Some('+') => magnitude,
                _ => magnitude,
            };
            Ok((dice_part, modifier))
        }
        None => Ok((expression, 0)),
    }
}

fn parse_u32(part: &str, what: &str) -> Result<u32> {
    part.trim().parse().map_err(|_| {
        AppError::Domain(format!("{what} must be a positive integer"))
    })
}

/// Roll the dice with an injected RNG (deterministic under a seeded RNG).
pub fn roll_with<R: Rng>(
    spec: &DiceSpec,
    expression: &str,
    rng: &mut R,
) -> DiceRoll {
    let rolls: Vec<u32> =
        (0..spec.count).map(|_| rng.gen_range(1..=spec.sides)).collect();
    let sum: u32 = rolls.iter().sum();
    DiceRoll {
        expression: expression.to_string(),
        total: sum as i32 + spec.modifier,
        rolls,
    }
}

/// The `dice` game command: roll a validated dice expression.
pub struct DiceCommand;

impl GameCommand for DiceCommand {
    fn id(&self) -> &str {
        "dice"
    }

    fn description(&self) -> &str {
        "Roll dice for the game. Provide an expression like 2d6, 1d20+2, or 3d8-1. Use it whenever an action's outcome is uncertain or opposed."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "dice".to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "expr": {
                        "type": "string",
                        "description": "Dice expression, e.g. 2d6, 1d20+2, 3d8-1",
                    }
                },
                "required": ["expr"],
            }),
        }
    }

    fn execute(
        &self,
        arguments: Value,
        _context: &mut CommandContext,
    ) -> Result<Value> {
        let expression =
            arguments.get("expr").and_then(Value::as_str).ok_or_else(|| {
                AppError::Domain("dice requires an 'expr' string".to_string())
            })?;
        let spec = parse_dice_expression(expression)?;
        let mut rng = rand::thread_rng();
        let roll = roll_with(&spec, expression, &mut rng);
        tracing::debug!(expression, total = roll.total, "dice rolled");
        Ok(json!({
            "expression": roll.expression,
            "total": roll.total,
            "rolls": roll.rolls,
        }))
    }
}

/// The `update_world` game command: set a key in the persistent world state.
///
/// Does not touch storage — it declares a [`StateMutation`] that the turn flow
/// applies and audits (§4.6 of PLAN.md).
pub struct UpdateWorldCommand;

impl GameCommand for UpdateWorldCommand {
    fn id(&self) -> &str {
        "update_world"
    }

    fn description(&self) -> &str {
        "Set or update a persistent fact in the world state (e.g. room conditions, NPC status, flags). The value becomes the new truth for future turns."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "update_world".to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "A snake_case key describing the fact, e.g. 'room_state' or 'npc.guard_awake'",
                    },
                    "value": {
                        "description": "The fact's value (string, number, bool, object, array)",
                    }
                },
                "required": ["key", "value"],
            }),
        }
    }

    fn execute(
        &self,
        arguments: Value,
        context: &mut CommandContext,
    ) -> Result<Value> {
        let key = arguments
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                AppError::Domain(
                    "update_world requires a non-empty 'key'".to_string(),
                )
            })?;
        let value = arguments.get("value").cloned().ok_or_else(|| {
            AppError::Domain("update_world requires a 'value'".to_string())
        })?;

        context.mutations.push(StateMutation::SetWorldKey {
            key: key.to_string(),
            value: value.clone(),
        });
        tracing::debug!(key, "world mutation declared");
        Ok(json!({ "ok": true, "key": key, "value": value }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn parses_common_expressions() {
        assert_eq!(
            parse_dice_expression("2d6+1").unwrap(),
            DiceSpec { count: 2, sides: 6, modifier: 1 }
        );
        assert_eq!(
            parse_dice_expression("d20").unwrap(),
            DiceSpec { count: 1, sides: 20, modifier: 0 }
        );
        assert_eq!(
            parse_dice_expression("3d8-2").unwrap(),
            DiceSpec { count: 3, sides: 8, modifier: -2 }
        );
        assert_eq!(
            parse_dice_expression("  2D6  ").unwrap(),
            DiceSpec { count: 2, sides: 6, modifier: 0 }
        );
    }

    #[test]
    fn rejects_invalid_expressions() {
        assert!(parse_dice_expression("").is_err());
        assert!(parse_dice_expression("abc").is_err());
        assert!(parse_dice_expression("0d6").is_err());
        assert!(parse_dice_expression("2d0").is_err());
        assert!(parse_dice_expression("200d6").is_err());
        assert!(parse_dice_expression("2d2000").is_err());
        assert!(parse_dice_expression("2d6x").is_err());
    }

    #[test]
    fn rolls_within_bounds_and_applies_modifier() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let spec = parse_dice_expression("2d6+1").unwrap();
        for _ in 0..100 {
            let roll = roll_with(&spec, "2d6+1", &mut rng);
            assert_eq!(roll.rolls.len(), 2);
            assert!((3..=13).contains(&roll.total)); // 2..12 + 1
        }
    }

    #[test]
    fn update_world_declares_mutation_not_storage() {
        let command = UpdateWorldCommand;
        let mut context = CommandContext {
            campaign_id: "c1".to_string(),
            world: json!({}),
            mutations: Vec::new(),
        };
        let result = command
            .execute(
                json!({ "key": "room_state", "value": "flooded" }),
                &mut context,
            )
            .expect("valid args");
        assert_eq!(result["key"], "room_state");
        assert_eq!(result["value"], "flooded");
        assert_eq!(context.mutations.len(), 1);
        assert!(matches!(
            context.mutations.first(),
            Some(StateMutation::SetWorldKey { key, .. }) if key == "room_state"
        ));
    }

    #[test]
    fn update_world_rejects_missing_or_empty_key() {
        let command = UpdateWorldCommand;
        let mut context = CommandContext {
            campaign_id: "c1".to_string(),
            world: json!({}),
            mutations: Vec::new(),
        };
        assert!(command.execute(json!({ "value": 1 }), &mut context).is_err());
        assert!(command
            .execute(json!({ "key": "  ", "value": 1 }), &mut context)
            .is_err());
    }
}
