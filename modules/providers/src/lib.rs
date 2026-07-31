//! The `providers` module — LLM adapters, their config, and the registry.
//!
//! This is the only place provider-specific types may exist (§5.3 of AGENTS.md).
//! Two adapters ship in v1:
//! - [`mock::MockProvider`] — the reference implementation for tests and the
//!   fallback when no real provider is configured.
//! - [`openai::OpenAiCompatibleProvider`] — covers the planned OpenCode Go
//!   provider (`opencode-go/deepseek-v4-flash`) and any other OpenAI-compatible
//!   endpoint.
//!
//! API keys live in the OS keyring (or the `OPENCODE_API_KEY` env var), never
//! in the database (§5.4).

// Sub-modules compile unconditionally: they carry no Tauri dependency, so the
// adapter + registry logic stays testable headless (§5.11).
pub mod domain;
pub mod mock;
pub mod openai;
pub mod registry;
pub mod secrets;
pub mod service;
pub mod storage;

// Commands are feature-gated behind `tauri` so the module compiles and tests
// headless without dragging in the webview (and its crate graph, §5.11).
#[cfg(feature = "tauri")]
pub mod commands;

// Re-export the public surface so consumers import the module name rather than
// reaching into sub-modules; the internals stay private (§5.6).
pub use domain::{ProviderConfig, ProviderInfo, ProviderKind};
pub use mock::MockProvider;
pub use openai::OpenAiCompatibleProvider;
pub use registry::ProviderRegistry;
pub use secrets::Secrets;
pub use service::ProviderService;
