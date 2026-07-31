//! The `campaigns` module — each roleplay session is a campaign.
//!
//! A campaign is the root aggregate of the app (§5.1 of PLAN.md): it owns a
//! title, an optional ruleset, its settings, and (transitively) its characters,
//! messages, world state, and memories. Multiple independent campaigns are
//! supported from the start — each is a fully separate roleplay.
//!
//! Layout follows the module pattern from AGENTS.md §4: `domain` holds the pure
//! entity + validation, `storage` is the SQLite repo over `core::storage`,
//! `service` orchestrates, and `commands` (feature-gated `tauri`) is the thin
//! IPC layer.

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
pub use domain::{Campaign, NewCampaign, UpdateCampaign};
pub use service::CampaignService;
