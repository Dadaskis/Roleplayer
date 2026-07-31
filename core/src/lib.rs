//! The shared foundation of Roleplayer.
//!
//! This crate holds the *seams* every feature module stacks on — the traits and
//! shared types that let the app stay model-agnostic, storage-swappable, and
//! testable headlessly. It intentionally contains **no feature logic**: nothing
//! here knows about campaigns, turns, world rules, or providers beyond the
//! generic contracts they implement.
//!
//! The sacred seams (§5.3 of AGENTS.md):
//! - [`storage::Storage`] — the persistence boundary; SQLite is one impl.
//! - [`llm::LLMProvider`] — the model boundary; Mock is the reference impl.
//! - [`game_command::GameCommand`] — the tool-use boundary for GM actions.
//! - [`eventbus::EventBus`] — the decoupling bus between modules and the UI.
//!
//! Everything else in here is shared plumbing (errors, content-block types,
//! migrations) that modules compose with but never bolt feature logic onto.

pub mod errors;
pub mod eventbus;
pub mod game_command;
pub mod llm;
pub mod migrations;
pub mod storage;

// Re-export the most-used shared types at the crate root so modules write
// `roleplayer_core::Capabilities` instead of deep paths, and so the public
// surface is one stable list — the IPC contract and module imports depend on
// these names staying put.
pub use errors::{AppError, Result};
pub use llm::{
    Capabilities, ChatMessage, CompletionRequest, CompletionResponse,
    ContentBlock, LLMProvider, ModelInfo, Role, ToolSchema, Usage,
};

/// Creates a fresh random UUID (v4) for entity ids.
///
/// Ids are always generated backend-side, never trusted from clients
/// (§5.4 of AGENTS.md).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Current wall-clock time as an RFC3339 string, used for `created_at` /
/// `updated_at` columns so timestamps are human-readable and sortable.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
