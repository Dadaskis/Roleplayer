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
    /// How many dice to roll (1 when the count is omitted, e.g. "d20").
    pub count: u32,
    /// Face value of each die; samples land in [1, sides].
    pub sides: u32,
    /// Signed constant added to the sum (0 when no modifier is present).
    pub modifier: i32,
}

/// The result of a single roll.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiceRoll {
    /// The exact expression string the user/model asked for, echoed back.
    pub expression: String,
    /// Sum of all die faces plus the modifier; may dip below zero.
    pub total: i32,
    /// One entry per die, in roll order, so the UI can show each face.
    pub rolls: Vec<u32>,
}

/// Parse a dice expression. Format: `[count]d<sides>[+|-modifier]`.
///
/// Manual parser (no regex): split on the first `d`, then split the trailing
/// part on the first `+` or `-`. Everything is validated against sane bounds so
/// a hostile expression (huge count, absurd sides) fails with a typed error.
pub fn parse_dice_expression(expression: &str) -> Result<DiceSpec> {
    // Trim + lowercase so "2D6" and " 2d6 " parse identically; only digits
    // and sign characters matter, so casing is irrelevant.
    let expression = expression.trim().to_lowercase();
    if expression.is_empty() {
        // Nothing to parse; reject instead of silently treating "" as a roll.
        return Err(AppError::Domain(
            "dice expression must not be empty".to_string(),
        ));
    }

    // Peel the optional signed modifier off first so the remaining split on
    // 'd' sees a clean "[count]d<sides>" core.
    let (dice_part, modifier) = split_modifier(&expression)?;
    // The core must contain a 'd'; without it the input is not dice at all.
    let (count_part, sides_part) =
        dice_part.split_once('d').ok_or_else(|| {
            AppError::Domain(format!("not a dice expression: {expression}"))
        })?;

    // A missing count means "one die" (d20 == 1d20); the default lives here in
    // the domain so callers never pass an implicit zero count.
    let count = if count_part.is_empty() {
        1
    } else {
        parse_u32(count_part, "dice count")?
    };
    let sides = parse_u32(sides_part, "dice sides")?;

    // Bounds-check every component against the caps so a hostile expression
    // (200d9999+999999) cannot allocate huge vectors or skew later math (§5.10).
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

    // All three components are in range; the spec is ready to roll.
    Ok(DiceSpec { count, sides, modifier })
}

/// Split `2d6+1` into `("2d6", 1)`; handles `d20`, `2d6-2`, `2d6`.
fn split_modifier(expression: &str) -> Result<(&str, i32)> {
    // Locate the first '+' or '-' that appears after the 'd' so `-` in nothing
    // else can confuse the parse (dice parts never contain signs otherwise).
    let dice_index = expression.find('d').ok_or_else(|| {
        AppError::Domain(format!("not a dice expression: {expression}"))
    })?;
    // Only the trailing modifier may be signed; count/sides are digits only,
    // so the first sign strictly after 'd' is the modifier separator.
    // The search runs on the suffix starting AFTER the 'd', then re-adds the
    // 'd' offset so the resulting index points into the whole expression.
    let sign_index = expression[dice_index + 1..]
        .find(['+', '-'])
        .map(|offset| dice_index + 1 + offset);

    match sign_index {
        Some(index) => {
            // Everything before the sign is "[count]d<sides>"; everything after
            // is the modifier magnitude with its explicit sign.
            let dice_part = &expression[..index];
            // Grab the sign character itself; the char at `index` is the one the
            // search just found, so it is either '+' or '-'.
            let sign = expression[index..].chars().next();
            // Parse the magnitude as a plain integer; "2d6+1.5" or "2d6+abc"
            // fail here with a typed error naming the malformed modifier.
            let magnitude: i32 =
                expression[index + 1..].trim().parse().map_err(|_| {
                    AppError::Domain(
                        "dice modifier must be an integer".to_string(),
                    )
                })?;
            // Re-apply the sign; the `_` arm is unreachable (sign always came
            // from a +/- search), but it keeps the match total/exhaustive.
            let modifier = match sign {
                Some('-') => -magnitude,
                Some('+') => magnitude,
                _ => magnitude,
            };
            Ok((dice_part, modifier))
        }
        // No sign after the 'd': the whole expression is "[count]d<sides>"
        // with a zero modifier (e.g. "2d6", "d20").
        None => Ok((expression, 0)),
    }
}

/// Parse a non-negative integer sub-string, mapping any parse failure to a
/// domain error that names the offending part (count vs sides).
fn parse_u32(part: &str, what: &str) -> Result<u32> {
    // Trim so stray spaces around a token are tolerated; any parse failure is
    // mapped to a typed error that names which segment (count or sides) broke.
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
    // One sample per die in [1, sides]; gen_range includes both bounds.
    // Collecting into a Vec keeps the per-die faces visible in the result so
    // the UI can show exactly what was rolled.
    let rolls: Vec<u32> =
        (0..spec.count).map(|_| rng.gen_range(1..=spec.sides)).collect();
    // Sum the faces; this cannot overflow because count and sides are both
    // capped by the parse-time bounds (§5.10).
    let sum: u32 = rolls.iter().sum();
    DiceRoll {
        expression: expression.to_string(),
        // Cast to i32 so a negative modifier can push the total below the sum.
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
            // JSON Schema object the provider serializes into its own tool-call
            // format; "required" forces the model to always supply expr.
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
        // Model-supplied arguments are untrusted; extract `expr` strictly.
        let expression =
            arguments.get("expr").and_then(Value::as_str).ok_or_else(|| {
                AppError::Domain("dice requires an 'expr' string".to_string())
            })?;
        // Validate + parse before doing any work, so a bad expression is
        // rejected without consuming RNG entropy.
        let spec = parse_dice_expression(expression)?;
        // Real entropy for a user-facing roll; tests go through roll_with.
        let mut rng = rand::thread_rng();
        let roll = roll_with(&spec, expression, &mut rng);
        tracing::debug!(expression, total = roll.total, "dice rolled");
        // Return a plain JSON object; this is a tool *result* handed back to
        // the model, not a state mutation.
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
            // JSON Schema object; `value` has no "type" so the model can record
            // strings, numbers, bools, objects, or arrays (JSON first, §5.4).
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
        // Trim + reject blanks so a model cannot "write" to a whitespace key
        // that would be invisible in the world state and un-queryable.
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
        // The value is JSON by construction (the model produced it), so it is
        // cloned verbatim; no schema validation is applied at this boundary.
        let value = arguments.get("value").cloned().ok_or_else(|| {
            AppError::Domain("update_world requires a 'value'".to_string())
        })?;

        // Declare the mutation; the turn flow applies + audits it. Command
        // layer stays pure — no storage writes here (§5.2).
        context.mutations.push(StateMutation::SetWorldKey {
            key: key.to_string(),
            value: value.clone(),
        });
        tracing::debug!(key, "world mutation declared");
        // Echo the recorded fact back so the model sees the applied truth.
        Ok(json!({ "ok": true, "key": key, "value": value }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn parses_common_expressions() {
        // A count + positive modifier splits cleanly into all three fields.
        assert_eq!(
            parse_dice_expression("2d6+1").unwrap(),
            DiceSpec { count: 2, sides: 6, modifier: 1 }
        );
        // An omitted count defaults to one die.
        assert_eq!(
            parse_dice_expression("d20").unwrap(),
            DiceSpec { count: 1, sides: 20, modifier: 0 }
        );
        // A negative modifier carries its sign through to the spec.
        assert_eq!(
            parse_dice_expression("3d8-2").unwrap(),
            DiceSpec { count: 3, sides: 8, modifier: -2 }
        );
        // Leading/trailing whitespace and uppercase are normalized away.
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
        // A trailing non-digit after the sides is rejected by parse_u32.
        assert!(parse_dice_expression("2d6x").is_err());
    }

    #[test]
    fn rolls_within_bounds_and_applies_modifier() {
        // A seeded RNG keeps the test deterministic across runs.
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let spec = parse_dice_expression("2d6+1").unwrap();
        // Sample many rolls; every one must stay within [3, 13] = 2d6+1.
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
        // The result echoes the fact back to the model.
        assert_eq!(result["key"], "room_state");
        assert_eq!(result["value"], "flooded");
        // Crucially: one mutation was declared, and storage was never touched.
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
        // A missing key is rejected outright.
        assert!(command.execute(json!({ "value": 1 }), &mut context).is_err());
        // A whitespace-only key is trimmed then rejected as empty.
        assert!(command
            .execute(json!({ "key": "  ", "value": 1 }), &mut context)
            .is_err());
    }
}
