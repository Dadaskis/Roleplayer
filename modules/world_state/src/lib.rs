//! The `world_state` module — the single source of truth for a campaign's world.
//!
//! This is the anti-hallucination core (§4.6 of PLAN.md): the GM's "reality" is
//! a JSON document re-injected into the prompt every turn, and every mutation
//! lands in `state_changes` with a before/after snapshot so a hallucinated edit
//! is detectable and revertible. Commands never write here directly — they
//! return a mutation journal and the service applies it.

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
pub use domain::StateChange;
pub use service::WorldStateService;
