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

pub mod domain;
pub mod mock;
pub mod openai;
pub mod registry;
pub mod secrets;
pub mod service;
pub mod storage;

#[cfg(feature = "tauri")]
pub mod commands;

pub use domain::{ProviderConfig, ProviderInfo, ProviderKind};
pub use mock::MockProvider;
pub use openai::OpenAiCompatibleProvider;
pub use registry::ProviderRegistry;
pub use secrets::Secrets;
pub use service::ProviderService;
