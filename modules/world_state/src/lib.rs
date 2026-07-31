//! The `world_state` module — the single source of truth for a campaign's world.
//!
//! This is the anti-hallucination core (§4.6 of PLAN.md): the GM's "reality" is
//! a JSON document re-injected into the prompt every turn, and every mutation
//! lands in `state_changes` with a before/after snapshot so a hallucinated edit
//! is detectable and revertible. Commands never write here directly — they
//! return a mutation journal and the service applies it.

pub mod domain;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::StateChange;
pub use service::WorldStateService;
