//! The `search` module — full-text search over the transcript (FTS5).
//!
//! The `messages_fts` virtual table is kept in sync by triggers in the schema.
//! Search here is scoped to a campaign and returns typed content blocks, so the
//! UI can render results like messages.

pub mod domain;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::SearchResult;
pub use service::SearchService;
