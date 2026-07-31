//! Provider configuration types — the non-secret parts of a provider setup.
//!
//! Keys are deliberately *not* here: they live in the OS keyring (§5.4).

use roleplayer_core::llm::ModelInfo;
use serde::{Deserialize, Serialize};

/// Which adapter family a config belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// The reference test adapter.
    Mock,
    /// Any OpenAI-compatible `/chat/completions` endpoint (incl. OpenCode Go).
    OpenAiCompatible,
}

impl ProviderKind {
    /// Wire name used in the `provider_cfg` table.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Mock => "mock",
            ProviderKind::OpenAiCompatible => "openai_compatible",
        }
    }

    /// Inverse of [`ProviderKind::as_str`]; unknown kinds fall back to Mock so
    /// a corrupt row degrades instead of failing hard (§5.10).
    pub fn from_wire(value: &str) -> ProviderKind {
        match value {
            // The only non-default family currently on the wire.
            "openai_compatible" => ProviderKind::OpenAiCompatible,
            // Catch-all: any unrecognized string loads as the reference Mock
            // adapter rather than erroring a whole settings screen.
            _ => ProviderKind::Mock,
        }
    }
}

/// A stored provider configuration (no keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Backend-generated UUID (v4); clients never supply it (§5.4).
    pub id: String,
    /// Display name shown in the settings list.
    pub name: String,
    /// Which adapter family handles this config (drives the registry lookup).
    pub kind: ProviderKind,
    /// Endpoint root; openai_compatible configs point at a /v1 base.
    pub base_url: String,
    /// Default model id used when a turn does not override it.
    pub model: String,
    /// Exactly one config should hold the default flag for new campaigns.
    pub is_default: bool,
    /// RFC 3339 timestamp; configs list oldest first.
    pub created_at: String,
}

/// What the UI shows about a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Backend-generated UUID (v4); mirrors the stored config id.
    pub id: String,
    /// Display name shown in the settings list.
    pub name: String,
    /// Which adapter family handles this config.
    pub kind: ProviderKind,
    /// Endpoint root; shown so users can spot a wrong URL.
    pub base_url: String,
    /// Default model id used when a turn does not override it.
    pub model: String,
    /// Whether an API key is available (keyring or env) for this provider.
    pub has_key: bool,
    /// Exactly one config should hold the default flag for new campaigns.
    pub is_default: bool,
}

/// The default endpoint for OpenCode Go (OpenAI-compatible chat completions).
pub const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";

/// The default model the user runs under their OpenCode Go subscription.
pub const OPENCODE_GO_DEFAULT_MODEL: &str = "deepseek-v4-flash";

/// The env var that supplies the OpenCode Go API key.
pub const OPENCODE_API_KEY_ENV: &str = "OPENCODE_API_KEY";

/// Static fallback model catalog for OpenCode Go.
///
/// Used when the `/models` endpoint is unreachable, so the picker always has
/// something to show (§5.17: degrade gracefully, never fail hard).
pub fn opencode_go_known_models() -> Vec<ModelInfo> {
    // A hand-maintained catalog; each entry names the wire id, a friendly label,
    // and the advertised context/output/tooling so the UI can present real info
    // without a live /models call.
    vec![
        ModelInfo {
            id: "deepseek-v4-flash".to_string(),
            name: "DeepSeek V4 Flash".to_string(),
            context_window: Some(1_000_000),
            max_output: Some(384_000),
            supports_tools: true,
        },
        ModelInfo {
            id: "deepseek-v4-pro".to_string(),
            name: "DeepSeek V4 Pro".to_string(),
            context_window: Some(1_000_000),
            max_output: Some(384_000),
            supports_tools: true,
        },
        ModelInfo {
            id: "kimi-k2.7-code".to_string(),
            name: "Kimi K2.7 Code".to_string(),
            context_window: Some(256_000),
            max_output: Some(32_000),
            supports_tools: true,
        },
        ModelInfo {
            id: "glm-5.2".to_string(),
            name: "GLM-5.2".to_string(),
            context_window: Some(200_000),
            max_output: Some(32_000),
            supports_tools: true,
        },
        ModelInfo {
            id: "qwen3.7-max".to_string(),
            name: "Qwen3.7 Max".to_string(),
            context_window: Some(256_000),
            max_output: Some(64_000),
            supports_tools: true,
        },
        ModelInfo {
            id: "grok-4.5".to_string(),
            name: "Grok 4.5".to_string(),
            context_window: Some(256_000),
            max_output: Some(64_000),
            supports_tools: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_wire_format() {
        // Serialize then parse back: the pair must be an identity for both
        // known kinds, proving as_str/from_wire are inverses.
        for kind in [ProviderKind::Mock, ProviderKind::OpenAiCompatible] {
            assert_eq!(ProviderKind::from_wire(kind.as_str()), kind);
        }
    }

    #[test]
    fn unknown_kind_falls_back_to_mock() {
        // A value that matches no arm must degrade to the Mock reference
        // adapter instead of panicking (§5.10).
        assert_eq!(ProviderKind::from_wire("garbage"), ProviderKind::Mock);
    }
}
