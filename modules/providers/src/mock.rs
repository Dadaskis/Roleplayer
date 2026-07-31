//! The reference LLM provider implementation (§5.5 of AGENTS.md).
//!
//! Deterministic and headless-friendly: used by every test, the contract suite,
//! and as a fallback when no real provider is configured. If a change breaks
//! the Mock, it breaks the provider contract.
//!
//! The reply logic is pure and synchronous: given the same request it always
//! returns the same message, so assertions and contract tests stay stable.

use roleplayer_core::errors::Result;
use roleplayer_core::llm::{
    Capabilities, ChatMessage, CompletionRequest, CompletionResponse,
    ContentBlock, LLMProvider, ModelInfo, Role, Usage,
};
use serde_json::json;

/// How much of the user's text the mock echoes back (keeps replies short).
///
/// Bounded so a long paste from the user never produces an enormous mock
/// reply; the cap is applied in characters (see `chars().take` below).
const ECHO_LIMIT: usize = 120;

/// The reference adapter. Deterministic behaviour:
/// - If the last user message mentions "dice" (and tools are offered and no
///   tool result is present yet), it emits a `dice` tool call.
/// - Otherwise it replies with the template, `{user}` replaced by the (short)
///   last user text.
pub struct MockProvider {
    /// Provider id (normally "mock").
    ///
    /// Must match what the registry is keyed by for lookups to succeed.
    id: String,
    /// The model name it claims to be.
    model: String,
    /// Reply template; `{user}` is substituted with the last user text.
    reply_template: String,
    /// Whether to simulate tool calls on a "dice" trigger.
    simulate_tools: bool,
}

impl MockProvider {
    /// Create a mock with a reply template and tool simulation enabled.
    ///
    /// Params: `id` is the registry key, `model` the claimed model name,
    /// `reply_template` the fixed narration prefix. Tool simulation is always
    /// enabled; no variant disables it (tests rely on the fixed behaviour).
    pub fn new(id: &str, model: &str, reply_template: &str) -> MockProvider {
        // Copy each owned field in; callers can drop their borrowed strings.
        MockProvider {
            id: id.to_string(),
            model: model.to_string(),
            reply_template: reply_template.to_string(),
            // On by default so the reference provider is exercised in its
            // richest mode; a caller wanting no tools still gets the trigger
            // suppressed by offering no tools in the request instead.
            simulate_tools: true,
        }
    }

    /// Build the deterministic reply for a request.
    ///
    /// Rule 1 (dice trigger): if all four trigger conditions hold, return a
    /// fixed tool call so turn-flow tests can exercise the call → result
    /// round trip without a real model. Rule 2 (fallback): otherwise narrate
    /// the template plus a parenthesized, bounded echo of the last user text.
    fn build_reply(&self, request: &CompletionRequest) -> ChatMessage {
        // Resolve the most recent user input; newer GM/tool turns are skipped.
        let last_user_text = last_user_text(request);
        // A tool result in history means the tool ran; the mock then
        // narrates instead of re-triggering, or the dice loop never ends.
        // Without this guard, "roll dice" would emit a tool call in every
        // subsequent turn and the conversation would never advance.
        let tool_result_present =
            request.messages.iter().any(|message| message.role == Role::Tool);

        // Trigger rule: tools offered, no prior result, and the user says dice.
        // All four conditions must hold together; the lowercase match makes
        // "Dice" and "DICE" trigger identically.
        let should_call_dice = self.simulate_tools
            && !request.tools.is_empty()
            && !tool_result_present
            && last_user_text.to_lowercase().contains("dice");

        if should_call_dice {
            // Emit a fixed, valid dice call with hard-coded id/args so any
            // assertion on the round trip is reproducible across runs.
            return ChatMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall {
                    id: "mock-dice-1".to_string(),
                    tool: "dice".to_string(),
                    arguments: json!({ "expr": "2d6" }),
                }],
            };
        }

        // Non-trigger path: narrate, echoing the user's text in parentheses.
        // Trim by chars, not bytes, so multibyte text is never split in half.
        // A byte-indexed slice could land mid-character; char iteration
        // cannot, so this is the safe way to bound the echo.
        let echoed = if last_user_text.is_empty() {
            // No user text to echo; the reply is just the bare template.
            String::new()
        } else {
            let truncated: String =
                last_user_text.chars().take(ECHO_LIMIT).collect();
            // Parenthesized echo keeps the reply informative but compact.
            format!(" ({truncated})")
        };
        ChatMessage::text(
            Role::Assistant,
            // Template first, echo after; the echo's parentheses supply the
            // separator, so no extra glue is needed between the two.
            format!("{}{}", self.reply_template, echoed),
        )
    }
}

/// Last user message's text; scans backwards past newer GM/tool turns.
fn last_user_text(request: &CompletionRequest) -> String {
    // Iterate newest-first so a recent user turn wins over older ones even
    // when assistant/tool messages sit in between.
    request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User)
        .map(|message| {
            // User messages carry only text; join the blocks into one line.
            // Filtering keeps the method total: blocks of other kinds are
            // simply skipped rather than treated as an error.
            message
                .content
                .iter()
                .filter_map(ContentBlock::text)
                .collect::<Vec<&str>>()
                .join("\n")
        })
        // No user message at all (e.g. a bare system prompt) yields "", so
        // the echo path below stays valid instead of erroring.
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl LLMProvider for MockProvider {
    fn id(&self) -> &str {
        // Hand out a reference to the owned id; no copy is needed.
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        // Advertise everything: the mock simulates a fully capable provider
        // so feature paths (streaming, tools, json_mode) all get exercised.
        Capabilities {
            streaming: true,
            tool_use: true,
            json_mode: true,
            max_output_tokens: 4096,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // A single, fixed model — enough for the picker to render and tests
        // to assert against, with no network access required.
        Ok(vec![ModelInfo {
            id: self.model.clone(),
            name: "Mock Model".to_string(),
            context_window: Some(8192),
            max_output: Some(4096),
            supports_tools: true,
        }])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse> {
        // Same deterministic reply as streaming; this is the one-shot variant
        // that additionally reports synthetic usage and a fixed finish reason.
        let message = self.build_reply(&request);
        Ok(CompletionResponse {
            message,
            // Synthetic Usage keeps the non-streamed path honest for tests
            // that assert on token accounting.
            usage: Some(Usage::default()),
            // Always "stop": the mock never hits a token cap or tool loop.
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        on_delta: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<CompletionResponse> {
        let message = self.build_reply(&request);
        // Emit the text fragment by fragment so streaming UI logic is exercised.
        // Only the text content is streamed; a tool-call reply has no text
        // block, so nothing is emitted and the caller just gets the message.
        if let Some(text) = message.content.iter().find_map(ContentBlock::text)
        {
            // Split on spaces and re-add them so the UI reassembles the
            // sentence exactly; each callback is one streaming "token".
            for fragment in text.split(' ') {
                on_delta(format!("{fragment} "));
            }
        }
        Ok(CompletionResponse {
            message,
            usage: Some(Usage::default()),
            finish_reason: Some("stop".to_string()),
        })
    }
}

/// A fully static mock whose behaviour is fixed — used by contract tests so a
/// passing suite is reproducible regardless of local configuration.
pub fn contract_test_mock() -> MockProvider {
    // Fixed id/model/template: no contract test depends on ambient state.
    MockProvider::new("mock", "mock/model", "The GM nods thoughtfully.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::llm::{CompletionRequest, ToolSchema};

    fn request(messages: Vec<ChatMessage>) -> CompletionRequest {
        // Build a request that always offers the dice tool, so the trigger
        // conditions depend only on the message content in each test.
        CompletionRequest {
            model: "mock/model".to_string(),
            messages,
            tools: vec![ToolSchema {
                name: "dice".to_string(),
                description: "roll dice".to_string(),
                parameters: json!({}),
            }],
            temperature: None,
            max_tokens: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn replies_with_template_and_echo() {
        // No "dice" in the input → the fallback narration path runs.
        let mock =
            MockProvider::new("mock", "mock/model", "The tavern is quiet.");
        let response = mock
            .complete(request(vec![ChatMessage::text(
                Role::User,
                "I look around",
            )]))
            .await
            .expect("mock never fails");
        let text = response
            .message
            .content
            .iter()
            .find_map(ContentBlock::text)
            .expect("has text");
        // The reply is template first, user echo second.
        assert!(text.starts_with("The tavern is quiet."));
        assert!(text.contains("I look around"));
    }

    #[tokio::test]
    async fn triggers_dice_tool_call_on_request() {
        // "dice" in the last user message → the tool-call trigger fires.
        let mock = MockProvider::new("mock", "mock/model", "Roll!");
        let response = mock
            .complete(request(vec![ChatMessage::text(
                Role::User,
                "I swing, roll dice",
            )]))
            .await
            .expect("mock never fails");
        match response.message.content.first() {
            Some(ContentBlock::ToolCall { tool, .. }) => {
                assert_eq!(tool, "dice")
            }
            other => panic!("expected a dice tool call, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn narrates_after_tool_result() {
        // A full user → tool-call → tool-result round trip is in history; the
        // tool_result_present guard must stop the dice loop from re-triggering.
        let mock = MockProvider::new("mock", "mock/model", "The blow lands.");
        let messages = vec![
            ChatMessage::text(Role::User, "I swing, roll dice"),
            ChatMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall {
                    id: "1".to_string(),
                    tool: "dice".to_string(),
                    arguments: json!({ "expr": "2d6" }),
                }],
            },
            ChatMessage {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult {
                    id: "1".to_string(),
                    result: json!({ "total": 9 }),
                }],
            },
        ];
        let response =
            mock.complete(request(messages)).await.expect("mock never fails");
        let text = response
            .message
            .content
            .iter()
            .find_map(ContentBlock::text)
            .expect("has text");
        assert!(text.starts_with("The blow lands."));
    }
}
