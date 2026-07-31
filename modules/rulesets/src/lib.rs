//! The `rulesets` module — the GM's "brain" (§5.1 of PLAN.md).
//!
//! A ruleset is a reusable preset: the system prompt that defines how the GM
//! behaves and house rules. Campaigns reference a ruleset; the turn flow turns
//! it into the system message plus a world-state section.

pub mod domain;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::{
    NewRuleset, Ruleset, UpdateRuleset, DEFAULT_RULESET_NAME,
    DEFAULT_SYSTEM_PROMPT,
};
pub use service::RulesetService;
