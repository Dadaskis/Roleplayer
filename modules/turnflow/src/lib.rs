//! The `turnflow` module — the agentic GM loop and its game commands.
//!
//! This is where a "turn" happens: the player's action is bundled with the
//! ruleset, world state, characters, and recent history; the GM model streams a
//! reply; any tool calls (dice, world update) are executed with validation and
//! every world mutation lands in the `state_changes` audit trail (§4.6 of
//! PLAN.md). The transcript is owned here too (`messages` table).

pub mod domain;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::{DiceCommand, UpdateWorldCommand};
pub use service::{MessageDto, PreparedTurn, TurnService};
