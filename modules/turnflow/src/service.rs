//! Turn orchestration: the agentic GM loop that powers a single "turn".

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use roleplayer_campaigns::service::CampaignService;
use roleplayer_characters::service::CharacterService;
use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::eventbus::{AppEvent, EventBus};
use roleplayer_core::game_command::{CommandContext, GameCommand};
use roleplayer_core::llm::{
    ChatMessage, CompletionRequest, ContentBlock, Role, ToolSchema,
};
use roleplayer_core::storage::Storage;
use roleplayer_core::{new_id, now_rfc3339};
use roleplayer_providers::registry::ProviderRegistry;
use roleplayer_rulesets::domain::Ruleset;
use roleplayer_rulesets::service::RulesetService;
use roleplayer_world_state::service::WorldStateService;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::{DiceCommand, UpdateWorldCommand};
use crate::storage as repo;

/// How many transcript rows the context window includes (Phase 3 will make this
/// adaptive with memory; fixed now keeps every turn cheap).
const HISTORY_WINDOW: i64 = 40;

/// Maximum tool-loop iterations in one turn, so a misbehaving model can't loop
/// forever and burn tokens (§5.10, §5.17).
const MAX_TOOL_ITERATIONS: usize = 5;

/// A transcript row, in the provider-agnostic shape the UI and IPC use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub campaign_id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub model: Option<String>,
    pub turn_index: i64,
    pub created_at: String,
}

impl MessageDto {
    fn text(
        campaign_id: &str,
        role: Role,
        text: &str,
        turn_index: i64,
    ) -> MessageDto {
        MessageDto {
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            role,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            model: None,
            turn_index,
            created_at: now_rfc3339(),
        }
    }

    fn from_assistant(
        campaign_id: &str,
        message: &ChatMessage,
        model: &str,
        turn_index: i64,
    ) -> MessageDto {
        MessageDto {
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            role: message.role,
            content: message.content.clone(),
            model: Some(model.to_string()),
            turn_index,
            created_at: now_rfc3339(),
        }
    }
}

/// A turn that is prepared but not yet executed.
///
/// Spawning is deliberately the *caller's* job: running a turn in the
/// background requires a Tokio runtime, which only exists inside the app
/// (tauri's runtime). The service itself stays tauri-free, so it exposes
/// preparation and execution as two steps.
pub struct PreparedTurn {
    pub campaign_id: String,
    pub turn_index: i64,
    user_message: MessageDto,
}

/// Orchestrates the agentic GM loop (§4.6 of PLAN.md).
///
/// One instance per app, shared behind `Arc`. Execution is two-phase:
/// [`TurnService::prepare_turn`] validates and persists the user message
/// (cheap, synchronous), then the caller hands the [`PreparedTurn`] to
/// [`TurnService::run_prepared`] on the app's runtime so the UI stays
/// responsive while events stream (§5.12).
pub struct TurnService<S: Storage> {
    storage: Arc<S>,
    world: Arc<WorldStateService<S>>,
    campaigns: Arc<CampaignService<S>>,
    characters: Arc<CharacterService<S>>,
    rulesets: Arc<RulesetService<S>>,
    providers: Arc<ProviderRegistry>,
    bus: EventBus,
    commands: Vec<Arc<dyn GameCommand>>,
    /// Campaign ids with a pending cancel request.
    cancelled: Mutex<HashSet<String>>,
}

impl<S: Storage + 'static> TurnService<S> {
    /// Build the service with the v1 game commands (dice, world update).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        storage: Arc<S>,
        world: Arc<WorldStateService<S>>,
        campaigns: Arc<CampaignService<S>>,
        characters: Arc<CharacterService<S>>,
        rulesets: Arc<RulesetService<S>>,
        providers: Arc<ProviderRegistry>,
        bus: EventBus,
    ) -> TurnService<S> {
        TurnService {
            storage,
            world,
            campaigns,
            characters,
            rulesets,
            providers,
            bus,
            commands: vec![Arc::new(DiceCommand), Arc::new(UpdateWorldCommand)],
            cancelled: Mutex::new(HashSet::new()),
        }
    }

    /// Prepare a turn: validate, persist the user message, return the turn.
    ///
    /// Cheap and synchronous, safe to call from any thread (including a Tauri
    /// sync command thread with no Tokio runtime context). Execution is handed
    /// to [`TurnService::run_prepared`] by the caller on a real runtime.
    ///
    /// Deliberately does NOT spawn: `tokio::spawn` panics ("no reactor running")
    /// when called from a Tauri sync command thread, which has no runtime
    /// context. Spawning is the composition layer's job.
    pub fn prepare_turn(
        &self,
        campaign_id: &str,
        text: &str,
    ) -> Result<PreparedTurn> {
        let campaign = self.campaigns.get(campaign_id)?.ok_or_else(|| {
            AppError::Domain(format!("campaign not found: {campaign_id}"))
        })?;
        // Validate ruleset exists up front so a broken campaign fails fast
        // instead of mid-loop.
        self.resolve_ruleset(campaign.ruleset_id.as_deref())?;

        let turn_index =
            repo::latest_turn_index(self.storage.as_ref(), campaign_id)? + 1;
        let user_message =
            MessageDto::text(campaign_id, Role::User, text.trim(), turn_index);
        repo::insert_message(self.storage.as_ref(), &user_message)?;

        // Clear any stale cancel flag from a previous turn.
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.remove(campaign_id);
        }
        tracing::info!(campaign_id = %campaign_id, turn_index, "turn prepared");
        Ok(PreparedTurn {
            campaign_id: campaign_id.to_string(),
            turn_index,
            user_message,
        })
    }

    /// Execute a prepared turn on the caller's runtime (§5.12: off the UI
    /// thread). Events stream on the bus; this resolves when the turn ends.
    pub async fn run_prepared(self: Arc<Self>, prepared: PreparedTurn) {
        self.run_loop(
            prepared.campaign_id,
            prepared.user_message,
            prepared.turn_index,
        )
        .await;
    }

    /// Request cancellation of the running turn for a campaign.
    pub fn cancel_turn(&self, campaign_id: &str) {
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.insert(campaign_id.to_string());
        }
        tracing::info!(campaign_id = %campaign_id, "turn cancel requested");
    }

    /// Recent transcript rows for a campaign, oldest-first.
    pub fn list_messages(
        &self,
        campaign_id: &str,
        limit: i64,
    ) -> Result<Vec<MessageDto>> {
        repo::list_messages(self.storage.as_ref(), campaign_id, limit)
    }

    /// The agentic loop. Runs on a background task; never returns an error to a
    /// caller — failures are logged and surfaced as [`AppEvent::TurnError`].
    async fn run_loop(
        &self,
        campaign_id: String,
        user_message: MessageDto,
        turn_index: i64,
    ) {
        // Publish the user message so the UI shows it immediately.
        self.publish_turn_message(&user_message);

        // Build the initial provider conversation.
        let mut context =
            match self.build_context(&campaign_id, &user_message).await {
                Ok(context) => context,
                Err(error) => {
                    self.fail(&campaign_id, error);
                    return;
                }
            };

        let mut finished = false;
        for iteration in 1..=MAX_TOOL_ITERATIONS {
            if self.is_cancelled(&campaign_id) {
                tracing::info!(campaign_id = %campaign_id, "turn cancelled");
                return;
            }

            let response =
                match self.call_provider(&context, &campaign_id).await {
                    Ok(response) => response,
                    Err(error) => {
                        self.fail(&campaign_id, error);
                        return;
                    }
                };

            let model = self
                .providers
                .default_model()
                .unwrap_or_else(|| "unknown".to_string());
            let assistant = MessageDto::from_assistant(
                &campaign_id,
                &response.message,
                &model,
                turn_index,
            );
            repo::insert_message(self.storage.as_ref(), &assistant)
                .unwrap_or_else(|error| tracing::warn!(%error, "failed to persist assistant message"));
            self.publish_turn_message(&assistant);

            // Did the GM ask for tools?
            let tool_calls = response
                .message
                .content
                .iter()
                .filter(|block| matches!(block, ContentBlock::ToolCall { .. }))
                .count();

            if tool_calls == 0 {
                context.push(response.message);
                finished = true;
                break;
            }

            context.push(response.message.clone());
            let tool_messages = self
                .execute_tool_calls(
                    &campaign_id,
                    &response.message,
                    turn_index,
                    &model,
                )
                .await;
            match tool_messages {
                Ok(messages) => {
                    for tool_message in &messages {
                        if let Err(error) = repo::insert_message(
                            self.storage.as_ref(),
                            tool_message,
                        ) {
                            tracing::warn!(%error, "failed to persist tool result");
                        }
                        self.publish_turn_message(tool_message);
                        context.push(tool_message_to_chat(tool_message));
                    }
                }
                Err(error) => {
                    // A failed tool execution is not a failed turn — narrate the
                    // failure back to the model and keep going.
                    context.push(ChatMessage {
                        role: Role::Tool,
                        content: vec![ContentBlock::ToolResult {
                            id: new_id(),
                            result: json!({ "ok": false, "error": error.to_string() }),
                        }],
                    });
                }
            }
            tracing::info!(campaign_id = %campaign_id, iteration, "tool iteration complete");
        }

        if finished {
            self.bus.publish(AppEvent::TurnComplete {
                campaign_id: campaign_id.clone(),
                turn_index,
            });
            tracing::info!(campaign_id = %campaign_id, turn_index, "turn complete");
        } else if self.is_cancelled(&campaign_id) {
            tracing::info!(campaign_id = %campaign_id, "turn cancelled");
        } else {
            // Hit the iteration cap: degrade gracefully rather than erroring.
            self.bus.publish(AppEvent::TurnError {
                campaign_id: campaign_id.clone(),
                message: "tool loop exceeded its iteration cap".to_string(),
            });
            tracing::warn!(campaign_id = %campaign_id, "tool iteration cap reached");
        }
    }

    /// Resolve the campaign's ruleset (or the built-in default).
    fn resolve_ruleset(&self, ruleset_id: Option<&str>) -> Result<Ruleset> {
        let ruleset = match ruleset_id {
            Some(id) => self.rulesets.get(id)?.ok_or_else(|| {
                AppError::Domain(format!("ruleset not found: {id}"))
            })?,
            None => {
                let builtin = self
                    .rulesets
                    .list()?
                    .into_iter()
                    .find(|ruleset| ruleset.is_builtin);
                match builtin {
                    Some(ruleset) => ruleset,
                    None => {
                        return Err(AppError::Domain(
                            "no ruleset configured for campaign".to_string(),
                        ))
                    }
                }
            }
        };
        Ok(ruleset)
    }

    /// Build the full provider conversation: system, history, current action.
    async fn build_context(
        &self,
        campaign_id: &str,
        user_message: &MessageDto,
    ) -> Result<Vec<ChatMessage>> {
        let campaign = self.campaigns.get(campaign_id)?.ok_or_else(|| {
            AppError::Domain("campaign not found".to_string())
        })?;
        let ruleset = self.resolve_ruleset(campaign.ruleset_id.as_deref())?;
        let world_document = self.world.get_document(campaign_id)?;
        let characters = self.characters.list_for_campaign(campaign_id)?;
        let history = repo::recent_messages(
            self.storage.as_ref(),
            campaign_id,
            HISTORY_WINDOW,
        )?;

        let system =
            self.build_system_message(&ruleset, &world_document, &characters)?;
        let mut context = vec![system];
        for stored in history {
            context.push(ChatMessage {
                role: stored.role,
                content: stored.content,
            });
        }
        context.push(ChatMessage {
            role: user_message.role,
            content: user_message.content.clone(),
        });
        Ok(context)
    }

    /// Assemble the system prompt: ruleset + world state + characters + tools.
    fn build_system_message(
        &self,
        ruleset: &Ruleset,
        world_document: &serde_json::Value,
        characters: &[roleplayer_characters::domain::Character],
    ) -> Result<ChatMessage> {
        let mut sections = vec![ruleset.system_prompt.clone()];

        sections.push(format!(
            "\n## CURRENT WORLD STATE (this is the truth)\n{}",
            serde_json::to_string_pretty(world_document)
                .unwrap_or_else(|_| "{}".to_string())
        ));

        if !characters.is_empty() {
            let roster = characters
                .iter()
                .map(|character| {
                    format!(
                        "- {} [{}]: {}",
                        character.name,
                        if character.is_player { "player" } else { "npc" },
                        character.stats
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("\n## CHARACTERS\n{roster}"));
        }

        sections.push(
            "\n## AVAILABLE TOOLS (use them to change the world)".to_string(),
        );
        for command in &self.commands {
            sections.push(format!(
                "- {}: {}",
                command.id(),
                command.description()
            ));
        }
        sections.push(
            "\nRemember: narrative free text never changes the world. Only tool calls do."
                .to_string(),
        );

        Ok(ChatMessage::text(Role::System, sections.join("\n")))
    }

    /// Run one provider call (streamed when the provider supports it).
    async fn call_provider(
        &self,
        context: &[ChatMessage],
        campaign_id: &str,
    ) -> Result<roleplayer_core::llm::CompletionResponse> {
        let provider = self.providers.require_default()?;
        let capabilities = provider.capabilities();
        let model = self.providers.default_model().ok_or_else(|| {
            AppError::Config("no default model configured".to_string())
        })?;

        let tools: Vec<ToolSchema> = if capabilities.tool_use {
            self.commands.iter().map(|command| command.schema()).collect()
        } else {
            // Capability degradation (§5.5): no tool support, narrate only.
            vec![]
        };

        let request = CompletionRequest {
            model,
            messages: context.to_vec(),
            tools,
            temperature: None,
            max_tokens: Some(capabilities.max_output_tokens as u32),
            stream: capabilities.streaming,
        };

        if capabilities.streaming {
            let bus = self.bus.clone();
            let campaign_id = campaign_id.to_string();
            let on_delta = Box::new(move |delta: String| {
                bus.publish(AppEvent::TurnDelta {
                    campaign_id: campaign_id.clone(),
                    delta,
                });
            });
            provider.stream(request, on_delta).await
        } else {
            provider.complete(request).await
        }
    }

    /// Execute every tool call in an assistant message, applying + auditing any
    /// world mutations. Returns the tool-result chat messages.
    async fn execute_tool_calls(
        &self,
        campaign_id: &str,
        assistant: &ChatMessage,
        turn_index: i64,
        model: &str,
    ) -> Result<Vec<MessageDto>> {
        let mut results = Vec::new();
        for block in &assistant.content {
            let ContentBlock::ToolCall { id, tool, arguments } = block else {
                continue;
            };

            self.bus.publish(AppEvent::TurnToolCall {
                campaign_id: campaign_id.to_string(),
                tool: tool.clone(),
                arguments: arguments.clone(),
            });

            let command = match self
                .commands
                .iter()
                .find(|command| command.id() == tool)
            {
                Some(command) => command,
                None => {
                    tracing::warn!(campaign_id = %campaign_id, tool, "unknown tool call");
                    results.push(MessageDto {
                        id: new_id(),
                        campaign_id: campaign_id.to_string(),
                        role: Role::Tool,
                        content: vec![ContentBlock::ToolResult {
                            id: id.clone(),
                            result: json!({ "ok": false, "error": "unknown tool" }),
                        }],
                        model: Some(model.to_string()),
                        turn_index,
                        created_at: now_rfc3339(),
                    });
                    continue;
                }
            };

            let world_snapshot = self.world.get_document(campaign_id)?;
            let mut context = CommandContext {
                campaign_id: campaign_id.to_string(),
                world: world_snapshot,
                mutations: Vec::new(),
            };

            match command.execute(arguments.clone(), &mut context) {
                Ok(result) => {
                    // Apply + audit every declared mutation (§4.6).
                    if !context.mutations.is_empty() {
                        self.world
                            .apply_mutations(
                                campaign_id,
                                &context.mutations,
                                tool,
                                arguments,
                                None,
                            )
                            .unwrap_or_else(|error| {
                                tracing::error!(
                                    campaign_id = %campaign_id,
                                    tool,
                                    %error,
                                    "failed to apply world mutations"
                                );
                                Vec::new()
                            });
                    }
                    self.bus.publish(AppEvent::TurnToolResult {
                        campaign_id: campaign_id.to_string(),
                        tool: tool.clone(),
                        ok: true,
                    });
                    results.push(MessageDto {
                        id: new_id(),
                        campaign_id: campaign_id.to_string(),
                        role: Role::Tool,
                        content: vec![ContentBlock::ToolResult {
                            id: id.clone(),
                            result,
                        }],
                        model: Some(model.to_string()),
                        turn_index,
                        created_at: now_rfc3339(),
                    });
                }
                Err(error) => {
                    self.bus.publish(AppEvent::TurnToolResult {
                        campaign_id: campaign_id.to_string(),
                        tool: tool.clone(),
                        ok: false,
                    });
                    tracing::warn!(campaign_id = %campaign_id, tool, %error, "tool call failed");
                    results.push(MessageDto {
                        id: new_id(),
                        campaign_id: campaign_id.to_string(),
                        role: Role::Tool,
                        content: vec![ContentBlock::ToolResult {
                            id: id.clone(),
                            result: json!({ "ok": false, "error": error.to_string() }),
                        }],
                        model: Some(model.to_string()),
                        turn_index,
                        created_at: now_rfc3339(),
                    });
                }
            }
        }
        Ok(results)
    }

    fn publish_turn_message(&self, message: &MessageDto) {
        if let Ok(value) = serde_json::to_value(message) {
            self.bus.publish(AppEvent::TurnMessage {
                campaign_id: message.campaign_id.clone(),
                message: value,
            });
        }
    }

    fn fail(&self, campaign_id: &str, error: AppError) {
        tracing::error!(campaign_id = %campaign_id, %error, "turn failed");
        self.bus.publish(AppEvent::TurnError {
            campaign_id: campaign_id.to_string(),
            message: error.to_string(),
        });
    }

    fn is_cancelled(&self, campaign_id: &str) -> bool {
        self.cancelled
            .lock()
            .map(|set| set.contains(campaign_id))
            .unwrap_or(false)
    }
}

/// Convert a persisted tool-result row back into a provider chat message.
fn tool_message_to_chat(message: &MessageDto) -> ChatMessage {
    ChatMessage { role: Role::Tool, content: message.content.clone() }
}
