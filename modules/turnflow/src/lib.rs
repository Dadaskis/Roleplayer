//! The `turnflow` module — the agentic GM loop and its game commands.
//!
//! This is where a "turn" happens: the player's action is bundled with the
//! ruleset, world state, characters, and recent history; the GM model streams a
//! reply; any tool calls (dice, world update) are executed with validation and
//! every world mutation lands in the `state_changes` audit trail (§4.6 of
//! PLAN.md). The transcript is owned here too (`messages` table).

// Sub-modules compile unconditionally: domain/service/storage carry no Tauri
// dependency, so they stay testable headless (§5.11).
pub mod domain;
pub mod service;
pub mod storage;

// Commands are feature-gated behind `tauri` so the module compiles and tests
// headless without dragging in the webview (and its crate graph, §5.11).
#[cfg(feature = "tauri")]
pub mod commands;

// Re-export the public surface so consumers import the module name rather than
// reaching into sub-modules; the internals stay private (§5.6).
pub use domain::{DiceCommand, UpdateWorldCommand};
pub use service::{MessageDto, PreparedTurn, TurnService};
