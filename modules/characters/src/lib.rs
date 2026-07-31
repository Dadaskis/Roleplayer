//! The `characters` module — player and NPC sheets inside a campaign.
//!
//! Characters are the second-level aggregate: they belong to one campaign, hold
//! a free-form bio, a stats document (JSON, "any kind of data"), and an extra
//! document for anything a ruleset or module wants to attach.

pub mod domain;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::{Character, NewCharacter, UpdateCharacter};
pub use service::CharacterService;
