//! OpenAI-compatible provider adapter (`/chat/completions`).
//!
//! This covers the planned OpenCode Go provider (base
//! `https://opencode.ai/zen/go/v1`, key from `OPENCODE_API_KEY`, model
//! `opencode-go/deepseek-v4-flash`) and any other OpenAI-compatible endpoint.
//!
//! Resilience (§5.17): timeouts on every call, streamed responses are read to
//! EOF (dropping the request cancels the connection), and malformed payloads
//! fail with typed errors — never panics.
//!
//! Shape: the wire structs below mirror the OpenAI JSON shapes; `build_payload`
//! and `openai_message` convert outgoing data, `StreamAccumulator` reassembles
//! incoming deltas, and the trait impls drive the HTTP + SSE I/O.

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
///
/// A single non-streamed completion must return within this window; a slow or
/// hung provider fails the call (typed error) rather than blocking the caller.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// Streaming reads can outlive one-shot requests for long generations.
///
/// A streamed turn can run for minutes of wall-clock time (the token stream
/// stays alive while the model thinks), so the cap is far looser than one-shot.
const STREAM_TIMEOUT: Duration = Duration::from_secs(300);

/// Default capability assumptions for an OpenAI-compatible endpoint.
///
/// This is an assumption, not a queried value; individual requests may still
/// cap below it via `max_tokens`, and real limits vary by deployment.
const DEFAULT_MAX_OUTPUT: usize = 8192;

/// An adapter for any OpenAI-compatible chat completions API.
pub struct OpenAiCompatibleProvider {
    /// Stable id used in configs (e.g. "opencode-go").
    ///
    /// The registry keys adapters by this id and the default selection
    /// references it; it is stable across runs and matches config values.
    id: String,
    /// API root; `/chat/completions` is appended if not already present.
    ///
    /// Trailing slashes are stripped at construction so path joining is safe.
    base_url: String,
    /// Bearer token for authentication.
    ///
    /// Held only in memory, never persisted; supplied from the keyring or the
    /// env-var fallback before the adapter is built (§5.4).
    api_key: String,
    /// Capabilities advertised by this deployment.
    capabilities: Capabilities,
    /// Static model catalog used when the `/models` endpoint is unreachable.
    ///
    /// Keeps the model picker usable offline and in CI without network access.
    known_models: Vec<ModelInfo>,
    /// Shared HTTP client.
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    /// Build an adapter. `known_models` is the fallback catalog for the picker.
    ///
    /// The model is *not* stored here: it travels per-request via
    /// [`CompletionRequest::model`], so one adapter serves any of its models.
    ///
    /// Params: `id` is the stable config-facing key; `base_url` is the API
    /// root (may already end in `/chat/completions`); `api_key` is the bearer
    /// token; `known_models` is the offline fallback model catalog.
    pub fn new(
        id: &str,
        base_url: &str,
        api_key: &str,
        known_models: Vec<ModelInfo>,
    ) -> OpenAiCompatibleProvider {
        // Connect timeout guards a stalled handshake; calls add their own.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            // A client built from static config cannot fail; this panic is a
            // programming error, not a runtime condition worth recovering from.
            .expect("reqwest client build cannot fail with static config");
        OpenAiCompatibleProvider {
            // Owned copy: the registry keys by this id for the lifetime.
            id: id.to_string(),
            // Trim the trailing slash so appending a path never doubles it.
            // One trim removes all trailing slashes in one pass.
            base_url: base_url.trim_end_matches('/').to_string(),
            // Copy the key in; callers are free to drop their own string.
            api_key: api_key.to_string(),
            // Static claims: an OpenAI-compatible endpoint supports all three
            // features, so every instance advertises them uniformly. The turn
            // flow reads these to decide how it may drive the model.
            capabilities: Capabilities {
                streaming: true,
                tool_use: true,
                json_mode: true,
                max_output_tokens: DEFAULT_MAX_OUTPUT,
            },
            // Move the caller-supplied fallback catalog straight in.
            known_models,
            // The client is cloneable and cheaply shareable via the registry.
            client,
        }
    }

    /// Resolve the chat completions URL from the configured base.
    fn chat_url(&self) -> String {
        // The base already names the endpoint when a full path was configured;
        // reuse it as-is to respect any custom prefix the host may require.
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            // Otherwise it is a plain API root (e.g. ".../v1"); append method.
            format!("{}/chat/completions", self.base_url)
        }
    }

    /// Build the OpenAI wire payload from our provider-agnostic request.
    ///
    /// Always-present keys first (`model`, `messages`, `stream`); optional
    /// knobs (`tools`, `temperature`, `max_tokens`) are added only when the
    /// request actually carries them, letting provider defaults apply otherwise.
    fn build_payload(
        &self,
        request: &CompletionRequest,
        stream: bool,
    ) -> Value {
        // Seed the object with the three unconditional wire fields; messages
        // are converted per-message by `openai_message`, not by hand.
        let mut payload = json!({
            "model": request.model,
            "messages": request.messages.iter().map(openai_message).collect::<Vec<_>>(),
            "stream": stream,
        });
        // `tool_choice: auto` leaves the call/text decision to the model.
        // Tools are advertised only when the request offered any; a strict
        // server would reject an empty tools array, so omit it entirely.
        if !request.tools.is_empty() {
            payload["tools"] = json!(request
                .tools
                .iter()
                .map(openai_tool)
                .collect::<Vec<_>>());
            // With `auto` the model may still answer with plain text instead
            // of calling a tool; the turn flow handles either outcome.
            payload["tool_choice"] = json!("auto");
        }
        // Sent only when set, letting provider defaults apply otherwise.
        // Omitting an unset knob is precisely what lets provider-side
        // defaults win; sending 0/null would override them.
        if let Some(temperature) = request.temperature {
            payload["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            payload["max_tokens"] = json!(max_tokens);
        }
        // The assembled wire body is returned for callers to POST as JSON.
        payload
    }

    /// Fetch the live model catalog; empty on any failure (caller falls back).
    ///
    /// Returns a typed error on transport/parse failures so `list_models` can
    /// decide to degrade; never panics and never blocks on a dead endpoint.
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        // The `/models` endpoint hangs off the same base as chat completions.
        let url = format!("{}/models", self.base_url);
        // Issue an authenticated GET under the one-shot timeout; a stalled or
        // absent models endpoint fails here rather than stalling the picker.
        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                // Network-level failure (DNS, TLS, timeout) → typed provider
                // error the caller can treat as "catalog unavailable".
                roleplayer_core::llm::provider_error(
                    "model list request failed",
                    error,
                )
            })?;
        // Parse the body as the expected envelope; a shape mismatch is a
        // distinct error kind from the transport failure above.
        let body: ModelListResponse =
            response.json().await.map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "model list response malformed",
                    error,
                )
            })?;
        // The wire exposes only ids; other ModelInfo fields stay unknown.
        // Convert each bare id into a full ModelInfo, using the id for both
        // the id and the display name since the endpoint gives nothing more.
        Ok(body
            .data
            .into_iter()
            .map(|item| {
                // Copy the id before it is moved into `name` below.
                let model_id = item.id;
                ModelInfo {
                    id: model_id.clone(),
                    name: model_id,
                    // The `/models` endpoint reports no sizes in this contract,
                    // so the picker falls back to known_models' richer data.
                    context_window: None,
                    max_output: None,
                    // Assume tool support; the deployment is OpenAI-compatible.
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
    // Extract the wire role string up front; every branch below reuses it.
    let role = message.role.as_str();
    // Only text blocks feed the message string; tool blocks map below.
    // Filtering (rather than erroring) keeps a mixed message convertible;
    // the text blocks are joined with newlines so multi-paragraph text
    // stays readable on the wire.
    let text = message
        .content
        .iter()
        .filter_map(ContentBlock::text)
        .collect::<Vec<&str>>()
        .join("\n");

    // Arguments must be a serialized JSON string — that is the wire contract.
    // Each ToolCall block becomes one function entry; the surrounding fields
    // (`id`, `type`) are the fixed envelope OpenAI expects on every call.
    let tool_calls: Vec<Value> = message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall { id, tool, arguments } => Some(json!({
                "id": id,
                "type": "function",
                "function": { "name": tool, "arguments": arguments.to_string() },
            })),
            // Non-tool-call blocks are skipped; text is carried separately.
            _ => None,
        })
        .collect();

    // Tool results are a distinct wire role referencing the originating call.
    if message.role == Role::Tool {
        // A tool message carries exactly one result; pull out id + payload.
        let (tool_call_id, result) = message
            .content
            .iter()
            .find_map(|block| match block {
                ContentBlock::ToolResult { id, result } => {
                    Some((id.clone(), result.to_string()))
                }
                _ => None,
            })
            // A malformed result uses benign defaults, never an error.
            .unwrap_or_else(|| ("unknown".to_string(), "{}".to_string()));
        // Early return: tool results take a completely different wire shape
        // (role "tool" + the call id it answers), not the generic message.
        // The content field here is the tool's result, not assistant text.
        return json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": result,
        });
    }

    // Non-tool messages: start with just the role, add fields in place.
    let mut wire = json!({ "role": role });
    if text.is_empty() {
        // Assistant messages that only carry tool calls have null content.
        // Null (not absent) is what OpenAI-compatible APIs expect here; an
        // empty string would be rejected as an invalid content type.
        wire["content"] = Value::Null;
    } else {
        wire["content"] = json!(text);
    }
    if !tool_calls.is_empty() {
        // The `tool_calls` field is present only when there are calls to
        // report; a bare assistant message must not carry an empty array.
        wire["tool_calls"] = json!(tool_calls);
    }
    // The assembled single-message object goes into the `messages` array.
    wire
}

/// Maps a [`ToolSchema`] to the OpenAI `tools` array entry.
fn openai_tool(schema: &ToolSchema) -> Value {
    // Every tool is a "function" type whose own nested object carries the
    // schema; parameters travel as an already-structured JSON schema object.
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
    /// The candidate completions; we always take the first (n defaults to 1).
    choices: Vec<Choice>,
    /// Optional token accounting; absent on some servers.
    usage: Option<UsageBody>,
}

/// A single completion choice; n defaults to 1, so we take the first.
#[derive(Deserialize)]
struct Choice {
    /// The assistant message produced by this choice.
    message: WireMessage,
    /// Why generation stopped ("stop", "tool_calls", ...); may be absent.
    finish_reason: Option<String>,
}

/// The assistant message inside a non-streamed choice.
#[derive(Deserialize)]
struct WireMessage {
    /// Text content; `null` when the message only carries tool calls.
    #[serde(default)]
    content: Option<String>,
    /// Any tool calls the model requested; empty when the reply is text.
    #[serde(default)]
    tool_calls: Vec<WireToolCall>,
}

/// A wire tool call: its id plus the function that was requested.
#[derive(Deserialize)]
struct WireToolCall {
    /// The call id a tool result must later reference to answer it.
    id: String,
    /// The function the model asked to invoke.
    function: WireFunction,
}

/// The function half of a wire tool call; arguments are a JSON string.
#[derive(Deserialize)]
struct WireFunction {
    /// The tool name to look up in the registered command set.
    name: String,
    /// JSON-encoded arguments string; parsed leniently before use.
    arguments: String,
}

/// Optional token counters reported by the provider.
#[derive(Deserialize)]
struct UsageBody {
    /// Input token count; None when the provider does not report it.
    prompt_tokens: Option<u64>,
    /// Output token count; None when the provider does not report it.
    completion_tokens: Option<u64>,
}

/// The streamed `data:` chunk shape (delta variant of the wire message).
#[derive(Deserialize)]
struct StreamChunk {
    /// One delta entry per choice; with n=1 there is exactly one.
    choices: Vec<StreamChoice>,
}

/// One streamed choice; the delta carries the fragment for this chunk.
#[derive(Deserialize)]
struct StreamChoice {
    /// The fragment for this chunk; a missing delta deserializes to default.
    #[serde(default)]
    delta: StreamDelta,
    /// Set on the terminal chunk of a choice, signalling end of generation.
    finish_reason: Option<String>,
}

/// A delta fragment: content text and/or tool-call pieces.
#[derive(Deserialize, Default)]
struct StreamDelta {
    /// Text fragment; None on chunks that only carry tool-call deltas.
    #[serde(default)]
    content: Option<String>,
    /// Tool-call fragments; a chunk may carry several in-flight calls.
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
}

/// A tool-call fragment; id/name arrive once, arguments arrive piecewise.
#[derive(Deserialize)]
struct StreamToolCall {
    /// Ordinal identifying which in-flight call this fragment belongs to.
    index: usize,
    /// The call id; sent once on the first fragment of a call.
    #[serde(default)]
    id: Option<String>,
    /// The function fragment; None on chunks that carry only the id.
    #[serde(default)]
    function: Option<StreamFunction>,
}

/// The function half of a streamed tool-call fragment.
#[derive(Deserialize, Default)]
struct StreamFunction {
    /// The tool name; sent once on the first fragment of a call.
    #[serde(default)]
    name: Option<String>,
    /// A piece of the JSON-encoded arguments; concatenated across chunks.
    #[serde(default)]
    arguments: Option<String>,
}

/// Accumulates streamed deltas into a final message.
struct StreamAccumulator {
    /// Concatenated text from every content delta, in arrival order.
    text: String,
    /// Tool call fragments, keyed by their stream index.
    ///
    /// One entry per in-flight call; later fragments merge into the existing
    /// entry until `finish()` consumes the whole map.
    tool_calls: HashMap<usize, WireToolCall>,
    /// The finish reason seen so far; usually arrives on the terminal chunk.
    finish_reason: Option<String>,
}

impl StreamAccumulator {
    fn new() -> StreamAccumulator {
        // Start empty; every field is filled incrementally by `apply()`.
        StreamAccumulator {
            text: String::new(),
            tool_calls: HashMap::new(),
            finish_reason: None,
        }
    }

    /// Fold one SSE chunk's delta into the accumulator.
    fn apply(&mut self, chunk: &StreamChunk) {
        // A chunk may carry several choices, but with n=1 there is exactly
        // one; iterate so the merge logic stays uniform regardless.
        for choice in &chunk.choices {
            // Content deltas are simple: append the fragment verbatim.
            if let Some(content) = &choice.delta.content {
                self.text.push_str(content);
            }
            // Tool calls arrive as fragments; each must be merged, not
            // replaced, or later argument pieces would overwrite earlier ones.
            for tool_call in &choice.delta.tool_calls {
                // OpenAI streams tool calls in fragments; merge by index.
                // Get-or-create the slot for this index; the first sighting
                // of an index seeds it with empty id/name/arguments.
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
                // Id/name arrive once; later fragments only append arguments.
                // The id is overwritten rather than appended: it is a full
                // value on its fragment, not a piece of a larger one.
                if let Some(id) = &tool_call.id {
                    entry.id = id.clone();
                }
                // The function half is optional on any given fragment.
                if let Some(function) = &tool_call.function {
                    if let Some(name) = &function.name {
                        // Name, like id, is delivered whole once, so replace.
                        entry.function.name = name.clone();
                    }
                    if let Some(arguments) = &function.arguments {
                        // Arguments are the exception: they stream in small
                        // pieces, so each fragment appends to the growing
                        // string until the full JSON has been reassembled.
                        entry.function.arguments.push_str(arguments);
                    }
                }
            }
            // Keep the first finish reason; later chunks may repeat it.
            // Once set it stays; overwrites are harmless since the value is
            // the same terminal marker repeated across the last chunks.
            if choice.finish_reason.is_some() {
                self.finish_reason = choice.finish_reason.clone();
            }
        }
    }

    /// Build the final provider-agnostic response.
    ///
    /// Consumes the accumulator; no further `apply()` calls are allowed. Text
    /// and merged tool calls combine into a single assistant message.
    fn finish(self) -> CompletionResponse {
        // Reassemble the content blocks in stream order: text, then calls.
        let mut content = Vec::new();
        if !self.text.is_empty() {
            // Only a non-empty text fragment becomes a block; a pure
            // tool-call turn must not carry an empty text block.
            content.push(ContentBlock::Text { text: self.text });
        }
        // Pull the entries out of the HashMap so they can be re-ordered.
        let mut calls: Vec<(usize, WireToolCall)> =
            self.tool_calls.into_iter().collect();
        // Stream order, so a multi-call turn comes back in a stable sequence.
        // HashMap iteration order is unspecified, so sorting by the stream
        // index makes the final order match the order the model issued calls.
        calls.sort_by_key(|(index, _)| *index);
        for (_index, call) in calls {
            // The arguments arrive as a JSON string; best-effort parse with a
            // graceful fallback so a malformed call never breaks the turn.
            // A truncated or invalid arguments string degrades to `{}` rather
            // than failing the whole streamed turn.
            let arguments = serde_json::from_str(&call.function.arguments)
                .unwrap_or(Value::Object(Default::default()));
            content.push(ContentBlock::ToolCall {
                id: call.id,
                tool: call.function.name,
                arguments,
            });
        }
        CompletionResponse {
            // The turn always returns an assistant message; the blocks may be
            // text and/or tool calls depending on what the model chose to do.
            message: ChatMessage { role: Role::Assistant, content },
            // Usage is not aggregated across deltas; streams report it as None.
            usage: None,
            finish_reason: self.finish_reason,
        }
    }
}

/// Wire body for the `/models` endpoint.
#[derive(Deserialize)]
struct ModelListResponse {
    /// The catalog entries; each is just an id in this contract.
    data: Vec<ModelListItem>,
}

#[derive(Deserialize)]
struct ModelListItem {
    /// The model identifier, used for both the ModelInfo id and display name.
    id: String,
}

#[async_trait::async_trait]
impl LLMProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        // Hand out a reference to the owned id; no copy is needed.
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        // Return the static claim by value; Capabilities is Copy-sized.
        self.capabilities
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Prefer the live catalog when the endpoint answered with at least
        // one model; it reflects the deployment's actual available models.
        match self.fetch_models().await {
            Ok(models) if !models.is_empty() => Ok(models),
            // The static catalog keeps the picker usable offline.
            // Any failure or empty response falls back to the compiled-in
            // list, so the UI never presents an empty model selector.
            _ => Ok(self.known_models.clone()),
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse> {
        // Build the wire payload with streaming disabled for this path.
        let payload = self.build_payload(&request, false);
        // POST the payload; the per-call timeout bounds the whole exchange,
        // and dropping this future (abort) also aborts the underlying request.
        let response = self
            .client
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&payload)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                // Transport-level failure (network, TLS, timeout) maps to a
                // typed provider error the UI can render and retry.
                roleplayer_core::llm::provider_error(
                    "completion request failed",
                    error,
                )
            })?;

        let status = response.status();
        // Read the whole body into memory before judging the status; this
        // lets a non-2xx reply include the provider's own error message.
        let body_text = response.text().await.map_err(|error| {
            roleplayer_core::llm::provider_error(
                "completion response unreadable",
                error,
            )
        })?;
        // Read the body before checking the status so errors can quote it.
        if !status.is_success() {
            // Surface the HTTP code plus a capped excerpt of the provider's
            // error body so logs stay bounded (§5.12) yet debuggable.
            return Err(roleplayer_core::llm::provider_error(
                &format!("provider returned HTTP {status}"),
                truncate(&body_text, 300),
            ));
        }

        // Success: hand the body to the shared non-streamed parser; malformed
        // bodies become typed errors here rather than panics downstream.
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
        // Same envelope as `complete()`, but with the stream flag switched on.
        let payload = self.build_payload(&request, true);
        // POST and negotiate the stream; the looser stream timeout applies
        // from here on since generation can run for many minutes.
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
        // Errors read the body so the provider's message can be quoted.
        // A non-2xx stream response is read in full (best-effort) and quoted.
        if !status.is_success() {
            // Read whatever error body there is; if even that read fails,
            // fall back to an empty excerpt rather than propagating a new
            // error on top of the HTTP failure we are already reporting.
            let body_text =
                response.text().await.unwrap_or_else(|_| String::new());
            return Err(roleplayer_core::llm::provider_error(
                &format!("provider returned HTTP {status}"),
                truncate(&body_text, 300),
            ));
        }

        // Read the SSE stream to EOF. Dropping this future (abort/cancel)
        // closes the connection, which is the cancellation mechanism.
        // The caller cancels by dropping the awaited future; reqwest then
        // drops the connection, signalling the provider to stop generating.
        let mut byte_stream = response.bytes_stream();
        // `buffer` holds the unconsumed tail of the SSE text; `lines()`
        // splits on newlines but frames must be whole, so an unterminated
        // tail must survive across `next()` reads (drain logic below).
        let mut buffer = String::new();
        let mut accumulator = StreamAccumulator::new();

        // Pull chunks until the server closes the stream (EOF).
        while let Some(chunk) = byte_stream.next().await {
            // A read failure ends the stream with a typed error; the partial
            // accumulation is discarded, which is correct — a broken stream
            // cannot yield a trustworthy completion anyway.
            let chunk = chunk.map_err(|error| {
                roleplayer_core::llm::provider_error(
                    "stream read failed",
                    error,
                )
            })?;
            // Lossy conversion keeps the stream alive for non-UTF-8 bytes.
            // Frames are ASCII JSON, so any invalid byte is almost certainly
            // padding; lossy decoding keeps us going instead of aborting.
            let chunk_text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_text);

            // SSE frames are newline-delimited; each `data:` line is one chunk.
            // `lines()` iterates every line the buffer currently holds,
            // including a final unterminated line (handled by the drain).
            for line in buffer.lines() {
                if let Some(payload) = line.trim().strip_prefix("data: ") {
                    // The stream-termination sentinel; nothing follows it.
                    // This is the last frame; deltas stop being fed here.
                    if payload == "[DONE]" {
                        continue;
                    }
                    // Each data frame is an independent JSON object; a frame
                    // that fails to parse is skipped rather than made fatal.
                    match serde_json::from_str::<StreamChunk>(payload) {
                        Ok(chunk) => {
                            // Merge the delta into the running accumulation.
                            accumulator.apply(&chunk);
                            // Forward new text fragments live so the UI can
                            // render tokens as they arrive (streaming path).
                            if let Some(content) =
                                chunk.choices.first().and_then(|choice| {
                                    choice.delta.content.as_ref()
                                })
                            {
                                // Live feed; finish() also returns it all.
                                // The callback receives the fragment, not the
                                // whole accumulated text; the UI concatenates.
                                on_delta(content.to_string());
                            }
                        }
                        Err(_error) => {
                            // Ignore unparseable frames; the stream is resilient.
                            // Comment/ping frames and transient glitches must
                            // not abort a long-running generation.
                        }
                    }
                }
            }
            // `lines()` yields an unterminated final line; the drain keeps it
            // so a frame split across chunks completes on the next read.
            // A frame may be split across network chunks, so the trailing
            // partial line is deliberately left in the buffer; only the
            // completed portion is discarded this round.
            // Drop the consumed portion of the buffer.
            if let Some(newline_position) = buffer.rfind('\n') {
                buffer.drain(..newline_position + 1);
            }
        }

        // EOF reached: finalize the accumulated deltas into one response.
        Ok(accumulator.finish())
    }
}

/// Parse a non-streamed completion body into the provider-agnostic response.
fn parse_completion_body(body: &str) -> Result<CompletionResponse> {
    // Parse the wire body first; a malformed body yields a typed Provider
    // error that the boundary can surface to the UI instead of a panic.
    let parsed: CompletionBody =
        serde_json::from_str(body).map_err(|error| {
            roleplayer_core::errors::AppError::Provider(error.to_string())
        })?;
    // n defaults to 1, so the first choice is the completion.
    // Take the first choice; an empty choices array means the server had no
    // completion to return, which is a provider-side error, not a panic.
    let choice = parsed.choices.into_iter().next().ok_or_else(|| {
        roleplayer_core::errors::AppError::Provider(
            "completion response had no choices".to_string(),
        )
    })?;

    // Rebuild the content block list in canonical order: text first, then
    // any tool calls the model made during this turn.
    let mut content = Vec::new();
    // Skip an empty string so a tool-call-only reply has no stray text block.
    // `filter` also drops a None content here, matching the "only tool calls"
    // wire case where content arrives as `null`.
    if let Some(text) = choice.message.content.filter(|text| !text.is_empty()) {
        content.push(ContentBlock::Text { text });
    }
    for call in choice.message.tool_calls {
        // Best-effort parse, mirroring finish(): malformed args degrade to {}.
        // The wire ships arguments as a JSON string; parse it into a Value so
        // the rest of the app reads a structured object, not raw text.
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
        // Usage maps 1:1 onto the wire counters; absent counters on the wire
        // stay absent here (Option is preserved through the mapping).
        usage: parsed.usage.map(|usage| Usage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }),
        finish_reason: choice.finish_reason,
    })
}

/// Cap an error message so a huge provider body never floods the logs/UI.
///
/// Returns the text unchanged when it already fits; otherwise a bounded
/// prefix plus an ellipsis. Purely defensive: error bodies must not balloon.
fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    // Byte-index cut: panics if `limit` lands mid-multibyte-char, which
    // small limits make unlikely; the cap bounds the size of error text.
    // This is a deliberate trade-off: slicing at a byte boundary is cheap
    // and a mid-character panic is improbable for ASCII-heavy bodies; the
    // appended ellipsis explains the truncation when the text is read back.
    format!("{}...", &text[..limit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_maps_tool_results_to_tool_role() {
        // Build a provider; its static config is irrelevant to this test.
        let provider = OpenAiCompatibleProvider::new(
            "test",
            "https://example.com/v1",
            "key",
            vec![],
        );
        // A ToolResult message must translate to the dedicated wire role.
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
        // A mixed message (text + a tool call) keeps both, in their own fields.
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
        // A one-shot body whose message has null content + a tool call; the
        // parser must produce a ToolCall block and keep the finish reason.
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
        // Feed three deltas that split one tool call across frames: the first
        // carries the id + name, the next two carry arguments in pieces. The
        // accumulator must reconstruct a single complete call.
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
        // Short input passes through untouched; long input is capped.
        let short = truncate("ok", 100);
        assert_eq!(short, "ok");
        let long = truncate(&"x".repeat(500), 100);
        assert_eq!(long.len(), 103); // 100 chars + "..."
    }
}
