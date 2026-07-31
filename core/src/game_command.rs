//! The game-command seam: how the GM mutates the world (§4.6 of PLAN.md).
//!
//! Commands are the *only* sanctioned way for the model to change game state.
//! A command receives validated arguments and a read-only world snapshot, and
//! returns its result plus a journal of [`StateMutation`]s. The turn flow
//! applies the journal to storage and writes the `state_changes` audit trail —
//! commands never touch storage themselves, which keeps them pure and keeps
//! the single source of truth under one writer.

use crate::errors::AppError;
use crate::llm::ToolSchema;
use serde::Serialize;
use serde_json::Value;

/// A mutation a command *wants* to apply. The turn flow is the only applier.
///
/// Keeping mutations declarative (instead of letting commands write directly)
/// is the anti-hallucination core: every world change is recorded with its
/// before/after snapshot in `state_changes` (§4.6 of PLAN.md).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateMutation {
    /// Set one key in the campaign's `world_state` document.
    SetWorldKey { key: String, value: Value },
}

/// Read-only execution context handed to a command.
///
/// `world` is a snapshot of the campaign's `world_state` document so commands
/// can read current facts without getting storage access.
#[derive(Debug)]
pub struct CommandContext {
    /// The campaign this turn belongs to.
    pub campaign_id: String,
    /// Snapshot of the `world_state` document at turn start.
    pub world: Value,
    /// Journal of mutations the command wants applied.
    pub mutations: Vec<StateMutation>,
}

/// A single in-world action the GM can call, e.g. dice or world update.
///
/// Implementations live in feature modules; the `turnflow` module registers
/// them with the provider. Adding a command is "implement + register", never
/// an edit to core (§5.3 of AGENTS.md).
///
/// `Send + Sync` is required because commands are invoked from the async turn
/// flow, which may run them on any worker thread while the registry is shared
/// behind an `Arc` across the whole runtime.
pub trait GameCommand: Send + Sync {
    /// Stable tool name the model uses to call it.
    fn id(&self) -> &str;

    /// One-line description shown to the model so it knows when to call it.
    fn description(&self) -> &str;

    /// JSON Schema of the arguments the command accepts.
    fn schema(&self) -> ToolSchema;

    /// Execute the command. Returns a JSON result for the model and pushes any
    /// mutations into `context.mutations`.
    fn execute(
        &self,
        arguments: Value,
        context: &mut CommandContext,
    ) -> Result<Value, AppError>;
}
