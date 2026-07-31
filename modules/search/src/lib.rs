//! The `search` module — full-text search over the transcript (FTS5).
//!
//! The `messages_fts` virtual table is kept in sync by triggers in the schema.
//! Search here is scoped to a campaign and returns typed content blocks, so the
//! UI can render results like messages.

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
pub use domain::SearchResult;
pub use service::SearchService;
