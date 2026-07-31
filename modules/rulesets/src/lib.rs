//! The `rulesets` module — the GM's "brain" (§5.1 of PLAN.md).
//!
//! A ruleset is a reusable preset: the system prompt that defines how the GM
//! behaves and house rules. Campaigns reference a ruleset; the turn flow turns
//! it into the system message plus a world-state section.

// Sub-modules compile unconditionally: domain/service/storage carry no Tauri
// dependency, so they stay testable headless (§5.11).
pub mod domain;
pub mod service;
pub mod storage;

// Commands are feature-gated behind `tauri` so the module compiles and tests
// headless without dragging in the webview (and its crate graph, §5.11).
#[cfg(feature = "tauri")]
pub mod commands;

// Re-export the public surface, including the seeded default constants the app
// crate uses on first run; internals stay private (§5.6).
pub use domain::{
    NewRuleset, Ruleset, UpdateRuleset, DEFAULT_RULESET_NAME,
    DEFAULT_SYSTEM_PROMPT,
};
pub use service::RulesetService;
