//! The model-agnostic LLM seam (§5.5 of AGENTS.md).
//!
//! Every provider adapter implements [`LLMProvider`]. All message content
//! travels as typed [`ContentBlock`] JSON — never provider-specific text — so
//! swapping a model is a config change, not a code change. The Mock provider is
//! the reference implementation: if a change breaks Mock, it breaks the contract.

use crate::errors::{AppError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who produced a message. Mirrors the standard chat-API roles plus `Tool` for
/// tool-call round-trips inside the agentic GM loop (§4.6 of PLAN.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    /// Stable wire name used in persistence and provider payloads.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }

    /// Inverse of [`Role::as_str`]; unknown strings are treated as `User` so
    /// malformed persisted rows degrade gracefully instead of failing hard.
    ///
    /// Named `from_wire` (not `from_str`) to avoid clashing with the standard
    /// `FromStr` trait that clippy flags as should-implement.
    pub fn from_wire(value: &str) -> Role {
        match value {
            "system" => Role::System,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::User,
        }
    }
}

/// One unit of message content in the provider-agnostic shape.
///
/// Tagged as `type: "text" | "tool_call" | "tool_result"` on the wire so the
/// stored transcript (a JSON array in `messages.content`) stays readable and
/// forward-compatible ("any kind of data", §5.4 of AGENTS.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Narrative text (GM prose, user actions, system directives).
    Text { text: String },
    /// A tool invocation requested by the model.
    ToolCall {
        /// Unique id so the matching tool result can be paired back.
        id: String,
        /// Which registered game command was invoked.
        tool: String,
        /// Validated argument JSON — parsed against the command schema.
        arguments: Value,
    },
    /// The outcome of a tool call, fed back to the model as a `Tool` message.
    ToolResult {
        /// Must match the id of the [`ContentBlock::ToolCall`] it answers.
        id: String,
        /// Result payload; `{"ok": false, "error": ...}` on failure.
        result: Value,
    },
}

impl ContentBlock {
    /// Plain text of a text block; `None` for tool blocks.
    pub fn text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A chat message: a role plus an ordered list of content blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl ChatMessage {
    /// Convenience constructor for a single text block.
    pub fn text(role: Role, text: impl Into<String>) -> ChatMessage {
        ChatMessage {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

/// JSON Schema description of a tool a provider may call.
///
/// Published by [`crate::game_command::GameCommand`]s and passed to the model so
/// it knows exactly what arguments a command accepts (§4.6 of PLAN.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    /// JSON Schema object (type, properties, required, ...).
    pub parameters: Value,
}

/// Capability flags a provider honestly advertises (§5.5 of AGENTS.md).
///
/// The app degrades based on these: no streaming -> one-shot, no tool-use ->
/// narrate-only GM, no json_mode -> lenient validation. Capabilities are the
/// contract that keeps the app working across wildly different providers.
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub streaming: bool,
    pub tool_use: bool,
    pub json_mode: bool,
    pub max_output_tokens: usize,
}

/// Metadata about a model a provider can run, surfaced in the picker UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_window: Option<usize>,
    pub max_output: Option<usize>,
    pub supports_tools: bool,
}

/// Token usage for a completion, when the provider reports it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

/// Everything a provider needs to produce a completion.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSchema>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

/// The completed turn: the assistant message plus usage metadata.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: ChatMessage,
    pub usage: Option<Usage>,
    pub finish_reason: Option<String>,
}

/// The model boundary. One implementation per provider family; implementations
/// live *only* inside the `providers` module (§5.3 of AGENTS.md).
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Stable provider id used in configs and the registry (e.g. "opencode-go").
    fn id(&self) -> &str;

    /// What this provider can honestly do; the app degrades around it.
    fn capabilities(&self) -> Capabilities;

    /// Models this provider can run, for the provider picker UI.
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// One-shot completion. Must apply its own timeout.
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse>;

    /// Streamed completion. `on_delta` is called with text fragments as they
    /// arrive (drives the streaming renderer); the full response is returned
    /// at the end. Must apply its own timeout and honour cancellation when the
    /// surrounding task is aborted (§5.17 of AGENTS.md).
    async fn stream(
        &self,
        request: CompletionRequest,
        on_delta: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<CompletionResponse>;
}

/// Maps an arbitrary HTTP/transport failure into the shared taxonomy.
pub fn provider_error(
    context: &str,
    error: impl std::fmt::Display,
) -> AppError {
    AppError::Provider(format!("{context}: {error}"))
}
