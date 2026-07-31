//! One shared error taxonomy for the whole app (§5.15 of AGENTS.md).
//!
//! Every layer maps lower-level errors up into this taxonomy, and the IPC
//! boundary converts it into a DTO the UI can render. No `rusqlite`, `reqwest`,
//! or provider-specific error type is ever allowed to leak past a seam.

use serde::Serialize;

/// The typed error kinds the app understands, grouped by concern.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Persistence layer failures (SQLite errors, migration failures).
    #[error("storage error: {0}")]
    Storage(String),

    /// LLM provider failures (network, timeouts, malformed responses).
    #[error("provider error: {0}")]
    Provider(String),

    /// Pure domain violations (invalid dice, unknown campaign, bad args).
    #[error("domain error: {0}")]
    Domain(String),

    /// Configuration / setup failures (missing key, bad settings).
    #[error("config error: {0}")]
    Config(String),

    /// IPC contract violations (bad command input from the UI).
    #[error("ipc error: {0}")]
    Ipc(String),
}

/// Result alias used across the app; services map their own errors into this.
pub type Result<T> = std::result::Result<T, AppError>;

/// Convert a `rusqlite` failure into the shared taxonomy.
///
/// The detail is flattened to a string on purpose: the `rusqlite` error type
/// must never leak past the storage seam (§5.3 of AGENTS.md), and the UI
/// boundary only renders the summary anyway.
impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        AppError::Storage(error.to_string())
    }
}

/// Convert a JSON parse failure into the shared taxonomy.
///
/// A `serde_json` error here means persisted or provider data was malformed
/// (untrusted input, §5.10) — a domain concern, not a storage or transport
/// one — so it is mapped to `Domain`.
impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::Domain(format!("invalid JSON data: {error}"))
    }
}

/// A user-facing error the UI can render — the *summary* of a logged detail.
///
/// This is the boundary DTO for failed Tauri commands (§5.15): the UI gets a
/// message it can show, a `retryable` flag, and the correlation id to look up
/// in the logs, never a raw stack trace.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorDto {
    /// Human-readable summary of what went wrong.
    pub message: String,
    /// Whether retrying the same operation is likely to help.
    pub retryable: bool,
    /// Correlation id that ties this failure to its log entries (§5.13).
    pub correlation_id: String,
}

/// Build the UI DTO from a taxonomy error.
///
/// Every failed command funnels through this: the raw error is what gets
/// logged (the detail), and the DTO is the only thing the UI ever sees.
impl From<AppError> for ErrorDto {
    fn from(error: AppError) -> Self {
        ErrorDto {
            message: error.to_string(),
            // Only provider errors are plausibly transient (network blips,
            // timeouts, rate limits). Retrying a storage/domain/ipc failure
            // would just repeat the same deterministic result.
            retryable: matches!(error, AppError::Provider(_)),
            // A fresh id per failure lets the user quote it and the logs be
            // grepped for that id to reconstruct the full context (§5.13).
            correlation_id: crate::new_id(),
        }
    }
}
