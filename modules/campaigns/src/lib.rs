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

pub mod domain;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::{Campaign, NewCampaign, UpdateCampaign};
pub use service::CampaignService;
