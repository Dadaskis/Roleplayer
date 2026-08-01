//! The `characters` module — player and NPC sheets inside a campaign.
//!
//! Characters are the second-level aggregate: they belong to one campaign, hold
//! a free-form bio, a stats document (JSON, "any kind of data"), and an extra
//! document for anything a ruleset or module wants to attach.

// Sub-modules compile unconditionally: domain/service/storage carry no Tauri
// dependency, so they stay testable headless (§5.11).
pub mod domain;
pub mod game_command;
pub mod service;
pub mod storage;

// Commands are feature-gated behind `tauri` so the module compiles and tests
// headless without dragging in the webview (and its crate graph, §5.11).
#[cfg(feature = "tauri")]
pub mod commands;

// Re-export the public surface so consumers import the module name rather than
// reaching into sub-modules; the internals stay private (§5.6).
pub use domain::{Character, NewCharacter, UpdateCharacter};
pub use game_command::CreateCharacterCommand;
pub use service::CharacterService;
