//! OpenAI-compatible provider adapter (`/chat/completions`).
//!
//! This covers the planned OpenCode Go provider (base
//! `https://opencode.ai/zen/go/v1`, key from `OPENCODE_API_KEY`, model
//! `opencode-go/deepseek-v4-flash`) and any other OpenAI-compatible endpoint.
//!
//! Resilience (§5.17): timeouts on every call, streamed responses are read to
//! EOF (dropping the request cancels the connection), and malformed payloads
//! fail with typed errors — never panics.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use roleplayer_core::errors::Result;
use roleplayer_core::llm::{
    Capabilities, ChatMessage, CompletionRequest, CompletionResponse,
    ContentBlock, LLMProvider, ModelInfo, Role, ToolSchema, Usage,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// Request timeout for one-shot completions.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// Streaming reads can outlive one-shot requests for long generations.
const STREAM_TIMEOUT: Duration = Duration::from_secs(300);

/// Default capability assumptions for an OpenAI-compatible endpoint.
const DEFAULT_MAX_OUTPUT: usize = 8192;

/// An adapter for any OpenAI-compatible chat completions API.
pub struct OpenAiCompatibleProvider {
    /// Stable id used in configs (e.g. "opencode-go").
    id: String,
    /// API root; `/chat/completions` is appended if not already present.
    base_url: String,
    /// Bearer token for authentication.
    api_key: String,
    /// Capabilities advertised by this deployment.
    capabilities: Capabilities,
    /// Static model catalog used when the `/models` endpoint is unreachable.
    known_models: Vec<ModelInfo>,
    /// Shared HTTP client.
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    /// Build an adapter. `known_models` is the fallback catalog for the picker.
    ///
    /// The model is *not* stored here: it travels per-request via
    /// [`CompletionRequest::model`], so one adapter serves any of its models.
    pub fn new(
        id: &str,
        base_url: &str,
        api_key: &str,
        known_models: Vec<ModelInfo>,
    ) -> OpenAiCompatibleProvider {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client build cannot fail with static config");
        OpenAiCompatibleProvider {
            id: id.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            capabilities: Capabilities {
                streaming: true,
                tool_use: true,
                json_mode: true,
                max_output_tokens: DEFAULT_MAX_OUTPUT,
            },
            known_models,
            client,
        }
    }

    /// Resolve the chat completions URL from the configured base.
    fn chat_url(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }

    /// Build the OpenAI wire payload from our provider-agnostic request.
    fn build_payload(
        &self,
        request: &CompletionRequest,
        stream: bool,
    ) -> Value {
        let mut payload = json!({
            "model": request.model,
            "messages": request.messages.iter().map(openai_message).collect::<Vec<_>>(),
            "stream": stream,
        });
        if !request.tools.is_empty() {
            payload["tools"] = json!(request
                .tools
                .iter()
                .map(openai_tool)
                .collect::<Vec<_>>());
            payload["tool_choice"] = json!("auto");
        }
        if let Some(temperature) = request.temperature {
            payload["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            payload["max_tokens"] = json!(max_tokens);
        }
        payload
    }

    /// Fetch the live model catalog; empty on any failure (caller falls back).
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "model list request failed",
                    error,
                )
            })?;
        let body: ModelListResponse =
            response.json().await.map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "model list response malformed",
                    error,
                )
            })?;
        Ok(body
            .data
            .into_iter()
            .map(|item| {
                let model_id = item.id;
                ModelInfo {
                    id: model_id.clone(),
                    name: model_id,
                    context_window: None,
                    max_output: None,
                    supports_tools: true,
                }
            })
            .collect())
    }
}

/// Maps a provider-agnostic message to the OpenAI wire shape.
///
/// Content is rendered as a string for text blocks; tool calls become the
/// structured `tool_calls` field; tool results become a `tool` role message
/// referencing the call id (required by OpenAI-compatible APIs).
fn openai_message(message: &ChatMessage) -> Value {
    let role = message.role.as_str();
    let text = message
        .content
        .iter()
        .filter_map(ContentBlock::text)
        .collect::<Vec<&str>>()
        .join("\n");

    let tool_calls: Vec<Value> = message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall { id, tool, arguments } => Some(json!({
                "id": id,
                "type": "function",
                "function": { "name": tool, "arguments": arguments.to_string() },
            })),
            _ => None,
        })
        .collect();

    if message.role == Role::Tool {
        let (tool_call_id, result) = message
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::ToolResult { id, result } => {
                    Some((id.clone(), result.to_string()))
                }
                _ => None,
            })
            .unwrap_or_else(|| ("unknown".to_string(), "{}".to_string()));
        return json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": result,
        });
    }

    let mut wire = json!({ "role": role });
    if text.is_empty() {
        // Assistant messages that only carry tool calls have null content.
        wire["content"] = Value::Null;
    } else {
        wire["content"] = json!(text);
    }
    if !tool_calls.is_empty() {
        wire["tool_calls"] = json!(tool_calls);
    }
    wire
}

/// Maps a [`ToolSchema`] to the OpenAI `tools` array entry.
fn openai_tool(schema: &ToolSchema) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": schema.name,
            "description": schema.description,
            "parameters": schema.parameters,
        },
    })
}

/// A non-streamed completion response body (the fields we care about).
#[derive(Deserialize)]
struct CompletionBody {
    choices: Vec<Choice>,
    usage: Option<UsageBody>,
}

#[derive(Deserialize)]
struct Choice {
    message: WireMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct WireMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireToolCall>,
}

#[derive(Deserialize)]
struct WireToolCall {
    id: String,
    function: WireFunction,
}

#[derive(Deserialize)]
struct WireFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct UsageBody {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

/// The streamed `data:` chunk shape (delta variant of the wire message).
#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
}

#[derive(Deserialize)]
struct StreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Deserialize, Default)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// Accumulates streamed deltas into a final message.
struct StreamAccumulator {
    text: String,
    /// Tool call fragments, keyed by their stream index.
    tool_calls: HashMap<usize, WireToolCall>,
    finish_reason: Option<String>,
}

impl StreamAccumulator {
    fn new() -> StreamAccumulator {
        StreamAccumulator {
            text: String::new(),
            tool_calls: HashMap::new(),
            finish_reason: None,
        }
    }

    /// Fold one SSE chunk's delta into the accumulator.
    fn apply(&mut self, chunk: &StreamChunk) {
        for choice in &chunk.choices {
            if let Some(content) = &choice.delta.content {
                self.text.push_str(content);
            }
            for tool_call in &choice.delta.tool_calls {
                // OpenAI streams tool calls in fragments; merge by index.
                let entry = self
                    .tool_calls
                    .entry(tool_call.index)
                    .or_insert_with(|| WireToolCall {
                        id: String::new(),
                        function: WireFunction {
                            name: String::new(),
                            arguments: String::new(),
                        },
                    });
                if let Some(id) = &tool_call.id {
                    entry.id = id.clone();
                }
                if let Some(function) = &tool_call.function {
                    if let Some(name) = &function.name {
                        entry.function.name = name.clone();
                    }
                    if let Some(arguments) = &function.arguments {
                        entry.function.arguments.push_str(arguments);
                    }
                }
            }
            if choice.finish_reason.is_some() {
                self.finish_reason = choice.finish_reason.clone();
            }
        }
    }

    /// Build the final provider-agnostic response.
    fn finish(self) -> CompletionResponse {
        let mut content = Vec::new();
        if !self.text.is_empty() {
            content.push(ContentBlock::Text { text: self.text });
        }
        let mut calls: Vec<(usize, WireToolCall)> =
            self.tool_calls.into_iter().collect();
        calls.sort_by_key(|(index, _)| *index);
        for (_index, call) in calls {
            // The arguments arrive as a JSON string; best-effort parse with a
            // graceful fallback so a malformed call never breaks the turn.
            let arguments = serde_json::from_str(&call.function.arguments)
                .unwrap_or(Value::Object(Default::default()));
            content.push(ContentBlock::ToolCall {
                id: call.id,
                tool: call.function.name,
                arguments,
            });
        }
        CompletionResponse {
            message: ChatMessage { role: Role::Assistant, content },
            usage: None,
            finish_reason: self.finish_reason,
        }
    }
}

/// Wire body for the `/models` endpoint.
#[derive(Deserialize)]
struct ModelListResponse {
    data: Vec<ModelListItem>,
}

#[derive(Deserialize)]
struct ModelListItem {
    id: String,
}

#[async_trait::async_trait]
impl LLMProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        match self.fetch_models().await {
            Ok(models) if !models.is_empty() => Ok(models),
            // The static catalog keeps the picker usable offline.
            _ => Ok(self.known_models.clone()),
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse> {
        let payload = self.build_payload(&request, false);
        let response = self
            .client
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&payload)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "completion request failed",
                    error,
                )
            })?;

        let status = response.status();
        let body_text = response.text().await.map_err(|error| {
            roleplayer_core::llm::provider_error(
                "completion response unreadable",
                error,
            )
        })?;
        if !status.is_success() {
            return Err(roleplayer_core::llm::provider_error(
                &format!("provider returned HTTP {status}"),
                truncate(&body_text, 300),
            ));
        }

        parse_completion_body(&body_text).map_err(|error| {
            roleplayer_core::llm::provider_error(
                "completion response malformed",
                error,
            )
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        on_delta: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<CompletionResponse> {
        let payload = self.build_payload(&request, true);
        let response = self
            .client
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&payload)
            .timeout(STREAM_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "stream request failed",
                    error,
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            let body_text =
                response.text().await.unwrap_or_else(|_| String::new());
            return Err(roleplayer_core::llm::provider_error(
                &format!("provider returned HTTP {status}"),
                truncate(&body_text, 300),
            ));
        }

        // Read the SSE stream to EOF. Dropping this future (abort/cancel)
        // closes the connection, which is the cancellation mechanism.
        let mut byte_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut accumulator = StreamAccumulator::new();

        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "stream read failed",
                    error,
                )
            })?;
            let chunk_text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_text);

            // SSE frames are newline-delimited; each `data:` line is one chunk.
            for line in buffer.lines() {
                if let Some(payload) = line.trim().strip_prefix("data: ") {
                    if payload == "[DONE]" {
                        continue;
                    }
                    match serde_json::from_str::<StreamChunk>(payload) {
                        Ok(chunk) => {
                            accumulator.apply(&chunk);
                            if let Some(content) =
                                chunk.choices.first().and_then(|choice| {
                                    choice.delta.content.as_ref()
                                })
                            {
                                on_delta(content.to_string());
                            }
                        }
                        Err(_error) => {
                            // Ignore unparseable frames; the stream is resilient.
                        }
                    }
                }
            }
            // Drop the consumed portion of the buffer.
            if let Some(newline_position) = buffer.rfind('\n') {
                buffer.drain(..newline_position + 1);
            }
        }

        Ok(accumulator.finish())
    }
}

/// Parse a non-streamed completion body into the provider-agnostic response.
fn parse_completion_body(body: &str) -> Result<CompletionResponse> {
    let parsed: CompletionBody =
        serde_json::from_str(body).map_err(|error| {
            roleplayer_core::errors::AppError::Provider(error.to_string())
        })?;
    let choice = parsed.choices.into_iter().next().ok_or_else(|| {
        roleplayer_core::errors::AppError::Provider(
            "completion response had no choices".to_string(),
        )
    })?;

    let mut content = Vec::new();
    if let Some(text) = choice.message.content.filter(|text| !text.is_empty()) {
        content.push(ContentBlock::Text { text });
    }
    for call in choice.message.tool_calls {
        let arguments = serde_json::from_str(&call.function.arguments)
            .unwrap_or(Value::Object(Default::default()));
        content.push(ContentBlock::ToolCall {
            id: call.id,
            tool: call.function.name,
            arguments,
        });
    }

    Ok(CompletionResponse {
        message: ChatMessage { role: Role::Assistant, content },
        usage: parsed.usage.map(|usage| Usage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }),
        finish_reason: choice.finish_reason,
    })
}

/// Cap an error message so a huge provider body never floods the logs/UI.
fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    format!("{}...", &text[..limit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_maps_tool_results_to_tool_role() {
        let provider = OpenAiCompatibleProvider::new(
            "test",
            "https://example.com/v1",
            "key",
            vec![],
        );
        let message = ChatMessage {
            role: Role::Tool,
            content: vec![ContentBlock::ToolResult {
                id: "call-1".to_string(),
                result: json!({ "total": 9 }),
            }],
        };
        let wire = openai_message(&message);
        assert_eq!(wire["role"], "tool");
        assert_eq!(wire["tool_call_id"], "call-1");
        assert_eq!(wire["content"], "{\"total\":9}");
        // The provider's static config is irrelevant here; silence the struct.
        assert_eq!(provider.id(), "test");
    }

    #[test]
    fn payload_maps_tool_calls_to_tool_calls_field() {
        let message = ChatMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text { text: "Rolling...".to_string() },
                ContentBlock::ToolCall {
                    id: "call-2".to_string(),
                    tool: "dice".to_string(),
                    arguments: json!({ "expr": "2d6" }),
                },
            ],
        };
        let wire = openai_message(&message);
        assert_eq!(wire["content"], "Rolling...");
        assert_eq!(wire["tool_calls"][0]["function"]["name"], "dice");
        assert_eq!(
            wire["tool_calls"][0]["function"]["arguments"],
            "{\"expr\":\"2d6\"}"
        );
    }

    #[test]
    fn parse_completion_handles_tool_calls() {
        let body = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call-3",
                        "type": "function",
                        "function": { "name": "update_world", "arguments": "{\"key\":\"room\",\"value\":\"flooded\"}" }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5 }
        }"#;
        let response =
            parse_completion_body(body).expect("parse should succeed");
        match response.message.content.first() {
            Some(ContentBlock::ToolCall { tool, arguments, .. }) => {
                assert_eq!(tool, "update_world");
                assert_eq!(arguments["key"], "room");
                assert_eq!(arguments["value"], "flooded");
            }
            other => panic!("expected tool call, got {other:?}"),
        }
        assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn stream_accumulator_merges_fragmented_tool_calls() {
        let mut accumulator = StreamAccumulator::new();
        accumulator.apply(&serde_json::from_str(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-9","function":{"name":"dice","arguments":""}}]}}]}"#,
        ).unwrap());
        accumulator.apply(&serde_json::from_str(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"expr\":\"2"}}]}}]}"#,
        ).unwrap());
        accumulator.apply(&serde_json::from_str(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"d6\"}"}}]}}]}"#,
        ).unwrap());
        let response = accumulator.finish();
        match response.message.content.first() {
            Some(ContentBlock::ToolCall { id, tool, arguments }) => {
                assert_eq!(id, "call-9");
                assert_eq!(tool, "dice");
                assert_eq!(arguments, &json!({ "expr": "2d6" }));
            }
            other => panic!("expected merged tool call, got {other:?}"),
        }
    }

    #[test]
    fn truncate_shortens_long_bodies() {
        let short = truncate("ok", 100);
        assert_eq!(short, "ok");
        let long = truncate(&"x".repeat(500), 100);
        assert_eq!(long.len(), 103); // 100 chars + "..."
    }
}
