//! The reference LLM provider implementation (§5.5 of AGENTS.md).
//!
//! Deterministic and headless-friendly: used by every test, the contract suite,
//! and as a fallback when no real provider is configured. If a change breaks
//! the Mock, it breaks the provider contract.

use roleplayer_core::errors::Result;
use roleplayer_core::llm::{
    Capabilities, ChatMessage, CompletionRequest, CompletionResponse,
    ContentBlock, LLMProvider, ModelInfo, Role, Usage,
};
use serde_json::json;

/// How much of the user's text the mock echoes back (keeps replies short).
const ECHO_LIMIT: usize = 120;

/// The reference adapter. Deterministic behaviour:
/// - If the last user message mentions "dice" (and tools are offered and no
///   tool result is present yet), it emits a `dice` tool call.
/// - Otherwise it replies with the template, `{user}` replaced by the (short)
///   last user text.
pub struct MockProvider {
    /// Provider id (normally "mock").
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
    pub fn new(id: &str, model: &str, reply_template: &str) -> MockProvider {
        MockProvider {
            id: id.to_string(),
            model: model.to_string(),
            reply_template: reply_template.to_string(),
            simulate_tools: true,
        }
    }

    /// Build the deterministic reply for a request.
    fn build_reply(&self, request: &CompletionRequest) -> ChatMessage {
        let last_user_text = last_user_text(request);
        let tool_result_present =
            request.messages.iter().any(|message| message.role == Role::Tool);

        let should_call_dice = self.simulate_tools
            && !request.tools.is_empty()
            && !tool_result_present
            && last_user_text.to_lowercase().contains("dice");

        if should_call_dice {
            return ChatMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall {
                    id: "mock-dice-1".to_string(),
                    tool: "dice".to_string(),
                    arguments: json!({ "expr": "2d6" }),
                }],
            };
        }

        let echoed = if last_user_text.is_empty() {
            String::new()
        } else {
            let truncated: String =
                last_user_text.chars().take(ECHO_LIMIT).collect();
            format!(" ({truncated})")
        };
        ChatMessage::text(
            Role::Assistant,
            format!("{}{}", self.reply_template, echoed),
        )
    }
}

fn last_user_text(request: &CompletionRequest) -> String {
    request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User)
        .map(|message| {
            message
                .content
                .iter()
                .filter_map(ContentBlock::text)
                .collect::<Vec<&str>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl LLMProvider for MockProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            streaming: true,
            tool_use: true,
            json_mode: true,
            max_output_tokens: 4096,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
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
        let message = self.build_reply(&request);
        Ok(CompletionResponse {
            message,
            usage: Some(Usage::default()),
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
        if let Some(text) = message.content.iter().find_map(ContentBlock::text)
        {
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
    MockProvider::new("mock", "mock/model", "The GM nods thoughtfully.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::llm::{CompletionRequest, ToolSchema};

    fn request(messages: Vec<ChatMessage>) -> CompletionRequest {
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
        assert!(text.starts_with("The tavern is quiet."));
        assert!(text.contains("I look around"));
    }

    #[tokio::test]
    async fn triggers_dice_tool_call_on_request() {
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
