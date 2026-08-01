//! Turn orchestration: the agentic GM loop that powers a single "turn".

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use roleplayer_campaigns::domain::CampaignStatus;
use roleplayer_campaigns::service::CampaignService;
use roleplayer_characters::domain::NewCharacter;
use roleplayer_characters::game_command::CreateCharacterCommand;
use roleplayer_characters::service::CharacterService;
use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::eventbus::{AppEvent, EventBus};
use roleplayer_core::game_command::{
    CommandContext, GameCommand, StateMutation,
};
use roleplayer_core::llm::{
    ChatMessage, CompletionRequest, ContentBlock, MessageMode, Role, ToolSchema,
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

/// Maximum tool-loop iterations in a normal turn, so a misbehaving model can't
/// loop forever and burn tokens (§5.10, §5.17).
const MAX_TOOL_ITERATIONS: usize = 5;

/// World generation gets its own, larger budget: the GM must build a world
/// document and a cast of characters before narrating the opening, which a
/// normal turn's cap cannot hold. The global constant is deliberately left
/// untouched so ordinary turns keep their tight safety bound.
const WORLDGEN_MAX_ITERATIONS: usize = 12;

/// Which mode a turn runs in. Selects the system prompt and the tool set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnPhase {
    /// Pre-world: the GM asks clarifying questions. No tools are exposed —
    /// the world does not exist yet, so nothing may be written.
    Setup,
    /// The GM generates the world and characters, then opens the story.
    Worldgen,
    /// Normal play: the agentic GM loop over a live world.
    Active,
}

/// A transcript row, in the provider-agnostic shape the UI and IPC use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    // Server-generated UUID; the persistence key of the transcript row.
    pub id: String,
    pub campaign_id: String,
    // Which side produced the row: user, assistant (GM), or tool result.
    pub role: Role,
    // Typed content blocks; text for narration, tool calls/results for the
    // agentic loop — never raw provider-specific text (§5.5).
    pub content: Vec<ContentBlock>,
    // For player rows: whether the message is dialogue (speech) or an action
    // (narration). GM/tool rows always store Action and never read it.
    pub mode: MessageMode,
    // The model that produced this row; None for user/tool rows.
    pub model: Option<String>,
    // Monotonic per-campaign sequence number; orders the transcript.
    pub turn_index: i64,
    // RFC3339 timestamp of when the row was created.
    pub created_at: String,
}

impl MessageDto {
    /// A plain text-only row (used for the user's typed action/speech).
    fn text(
        campaign_id: &str,
        role: Role,
        text: &str,
        mode: MessageMode,
        turn_index: i64,
    ) -> MessageDto {
        MessageDto {
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            role,
            // A single text block — there is no tooling on a user's message.
            content: vec![ContentBlock::Text { text: text.to_string() }],
            // The player's mode (action/speech) is the whole point of the row.
            mode,
            // User-typed rows carry no model provenance.
            model: None,
            turn_index,
            created_at: now_rfc3339(),
        }
    }

    /// Wrap a provider assistant message with model + turn metadata.
    fn from_assistant(
        campaign_id: &str,
        message: &ChatMessage,
        model: &str,
        turn_index: i64,
    ) -> MessageDto {
        MessageDto {
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            // Preserve whatever role the provider answered with (normally
            // Assistant), so the row faithfully reflects the response.
            role: message.role,
            // Clone the full block list — the same message is pushed into the
            // running context below, so the DTO must own its copy.
            content: message.content.clone(),
            // Mode is a player-only concept; GM rows store the default.
            mode: MessageMode::Action,
            // GM rows carry the model name for UI provenance display.
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
    // The persisted user message, replayed into the loop when run starts.
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
    // Shared seam for transcript persistence (messages are stored here).
    storage: Arc<S>,
    // World-state service: reads the document for context and applies tool
    // mutations with their audit trail.
    world: Arc<WorldStateService<S>>,
    // Campaign service: resolves the campaign owning this turn.
    campaigns: Arc<CampaignService<S>>,
    // Character service: loads the roster for the system prompt.
    characters: Arc<CharacterService<S>>,
    // Ruleset service: resolves the system prompt / GM instructions.
    rulesets: Arc<RulesetService<S>>,
    // Live provider adapter cache; the loop asks it for the default model.
    providers: Arc<ProviderRegistry>,
    // Typed event bus: streams deltas, messages, tool calls, and completion.
    bus: EventBus,
    // The registered GameCommand list the GM can invoke via tool calls.
    commands: Vec<Arc<dyn GameCommand>>,
    /// Campaign ids with a pending cancel request.
    cancelled: Mutex<HashSet<String>>,
    /// Campaign ids whose setup-intro turn is currently in flight (the
    /// idempotency guard that stops a double-invoked intro from running twice).
    intros_in_flight: Mutex<HashSet<String>>,
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
            // The tool set the GM may call: a read-only dice roll, the world
            // update, and character creation (worldgen builds the roster).
            // Adding a command is "implement GameCommand + register here"
            // (§8 of AGENTS.md); no core seam needs editing.
            commands: vec![
                Arc::new(DiceCommand),
                Arc::new(UpdateWorldCommand),
                Arc::new(CreateCharacterCommand),
            ],
            // Start with no pending cancellations; the set is empty per turn.
            cancelled: Mutex::new(HashSet::new()),
            // Start with no setup intros in flight.
            intros_in_flight: Mutex::new(HashSet::new()),
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
        mode: MessageMode,
    ) -> Result<PreparedTurn> {
        // Load the campaign first: the loop below needs it for the ruleset
        // and ownership checks, so fail fast if the id is bogus.
        let campaign = self.campaigns.get(campaign_id)?.ok_or_else(|| {
            AppError::Domain(format!("campaign not found: {campaign_id}"))
        })?;
        // A world is being generated right now; the player must not interleave
        // a message into that single-flight turn.
        if campaign.status == CampaignStatus::Worldgen {
            return Err(AppError::Domain(
                "the GM is generating the world; wait for it to finish"
                    .to_string(),
            ));
        }
        // The setup intro is a GM-initiated turn on this same campaign; the
        // player must not interleave a message into it either (it would race
        // for the same turn index). The intro is short — wait for its question.
        if self.intros_in_flight_set().contains(campaign_id) {
            return Err(AppError::Domain(
                "the GM is opening the session; wait for its question"
                    .to_string(),
            ));
        }
        // Validate ruleset exists up front so a broken campaign fails fast
        // instead of mid-loop.
        self.resolve_ruleset(campaign.ruleset_id.as_deref())?;

        // Compute the next index by asking the repo for the latest stored one;
        // this reserves a unique slot for this turn's rows.
        let turn_index =
            repo::latest_turn_index(self.storage.as_ref(), campaign_id)? + 1;
        // The user's trimmed text becomes the first transcript row, so it is
        // persisted even if the provider call later fails.
        let user_message = MessageDto::text(
            campaign_id,
            Role::User,
            text.trim(),
            mode,
            turn_index,
        );
        repo::insert_message(self.storage.as_ref(), &user_message)?;

        // Clear any stale cancel flag from a previous turn.
        if let Ok(mut cancelled) = self.cancelled.lock() {
            // A poisoned lock degrades to "leave as-is"; the next run will
            // still check is_cancelled and behave normally.
            cancelled.remove(campaign_id);
        }
        tracing::info!(campaign_id = %campaign_id, turn_index, "turn prepared");
        Ok(PreparedTurn {
            campaign_id: campaign_id.to_string(),
            turn_index,
            // Hand the prepared message back so the run phase replays it into
            // the conversation without a second DB read.
            user_message,
        })
    }

    /// Execute a prepared turn on the caller's runtime (§5.12: off the UI
    /// thread). Events stream on the bus; this resolves when the turn ends.
    pub async fn run_prepared(self: Arc<Self>, prepared: PreparedTurn) {
        // Consumes self via Arc so the loop can hold the service without the
        // caller keeping a reference alive across the await points.
        // The phase follows the campaign's lifecycle status: a campaign still
        // in setup answers in the Q&A mode, anything else plays normally.
        let phase = self.phase_for_campaign(&prepared.campaign_id);
        self.run_loop(
            &prepared.campaign_id,
            Some(&prepared.user_message),
            prepared.turn_index,
            phase,
            MAX_TOOL_ITERATIONS,
        )
        .await;
    }

    /// Resolve the turn phase from a campaign's current lifecycle status.
    fn phase_for_campaign(&self, campaign_id: &str) -> TurnPhase {
        // A missing campaign degrades to Active (the loop already surfaced the
        // missing-id error during prepare, so this is just a safe default).
        match self.campaigns.get(campaign_id) {
            Ok(Some(campaign)) if campaign.status == CampaignStatus::Setup => {
                TurnPhase::Setup
            }
            // A Worldgen campaign never reaches run_prepared — prepare_turn
            // rejects sends while the world is generating — so any other
            // status (including a stale read) plays with the Active prompt.
            _ => TurnPhase::Active,
        }
    }

    /// Start the setup-intro turn for a campaign, if one is due.
    ///
    /// "Due" means: the campaign is in `setup` AND has no transcript yet AND no
    /// intro is already in flight. Returns whether a turn was actually started
    /// (the caller then spawns [`TurnService::run_setup_intro`]).
    ///
    /// Idempotency: the in-flight marker is checked AND inserted under a single
    /// lock acquisition, so two racing invocations (StrictMode double mount,
    /// double-click) cannot both pass the guard — a TOCTOU gap would let two
    /// intro turns run concurrently on the same campaign.
    pub fn start_setup_intro(&self, campaign_id: &str) -> Result<bool> {
        // Hold one guard across the whole check-and-mark so no second caller
        // can slip between the contains() and insert().
        let mut in_flight = self.intros_in_flight_set();
        if in_flight.contains(campaign_id) {
            tracing::info!(campaign_id = %campaign_id, "setup intro already running");
            return Ok(false);
        }
        let campaign = self.campaigns.get(campaign_id)?.ok_or_else(|| {
            AppError::Domain(format!("campaign not found: {campaign_id}"))
        })?;
        // Only an untouched setup campaign gets the intro; a campaign with
        // messages (already played or intro done) is not due. These DB reads
        // happen while holding the guard — short, synchronous, and exactly
        // what serializes the decision.
        if campaign.status != CampaignStatus::Setup
            || repo::latest_turn_index(self.storage.as_ref(), campaign_id)? != 0
        {
            return Ok(false);
        }
        in_flight.insert(campaign_id.to_string());
        tracing::info!(campaign_id = %campaign_id, "setup intro starting");
        Ok(true)
    }

    /// Run the setup-intro turn (GM-initiated; no player message). Must be
    /// spawned on a runtime; removes the in-flight guard when it ends.
    pub async fn run_setup_intro(self: Arc<Self>, campaign_id: String) {
        // Reserve the next turn index for the GM's rows, like prepare_turn
        // does. A read failure means the transcript is unavailable — abort
        // rather than write at index 0 (which would collide with the
        // "no transcript yet" sentinel the intro guard relies on).
        let turn_index = match repo::latest_turn_index(
            self.storage.as_ref(),
            &campaign_id,
        ) {
            Ok(index) => index + 1,
            Err(error) => {
                self.fail(&campaign_id, error);
                self.intros_in_flight_set().remove(&campaign_id);
                return;
            }
        };
        self.run_loop(
            &campaign_id,
            None,
            turn_index,
            TurnPhase::Setup,
            MAX_TOOL_ITERATIONS,
        )
        .await;
        // Release the idempotency guard regardless of how the turn ended.
        self.intros_in_flight_set().remove(&campaign_id);
    }

    /// Start the world-generation flow: validate the campaign is in `setup`,
    /// flip it to the transient `worldgen` state, and report success. The
    /// caller then spawns [`TurnService::run_worldgen`].
    pub fn start_roleplay(&self, campaign_id: &str) -> Result<()> {
        let campaign = self.campaigns.get(campaign_id)?.ok_or_else(|| {
            AppError::Domain(format!("campaign not found: {campaign_id}"))
        })?;
        // Only a setup campaign may begin generation; worldgen/active no-op
        // via the guard, so a double-click cannot start two generation turns.
        if campaign.status != CampaignStatus::Setup {
            return Err(AppError::Domain(
                "campaign is not in the setup phase".to_string(),
            ));
        }
        // Flip to worldgen BEFORE the turn runs; this is the idempotency
        // mechanism — a second call now sees Worldgen and refuses.
        self.campaigns.set_status(campaign_id, CampaignStatus::Worldgen)?;
        tracing::info!(campaign_id = %campaign_id, "world generation starting");
        Ok(())
    }

    /// Run the worldgen turn, then settle the campaign's status.
    ///
    /// On a clean end (the GM narrated the opening) the campaign becomes
    /// `active`; on failure (tool-loop cap, provider error, cancel) it reverts
    /// to `setup` so the player can retry — never a half-generated world.
    pub async fn run_worldgen(self: Arc<Self>, campaign_id: String) {
        // Reserve the next turn index; on a read failure, revert the campaign
        // to setup and surface the error (never write at index 0, which would
        // corrupt the "no transcript yet" sentinel).
        let turn_index = match repo::latest_turn_index(
            self.storage.as_ref(),
            &campaign_id,
        ) {
            Ok(index) => index + 1,
            Err(error) => {
                let _ = self
                    .campaigns
                    .set_status(&campaign_id, CampaignStatus::Setup);
                self.fail(&campaign_id, error);
                return;
            }
        };
        let finished = self
            .run_loop(
                &campaign_id,
                None,
                turn_index,
                TurnPhase::Worldgen,
                WORLDGEN_MAX_ITERATIONS,
            )
            .await;
        // Settle the state machine based on how the generation turn ended.
        let next = if finished {
            CampaignStatus::Active
        } else {
            CampaignStatus::Setup
        };
        if let Err(error) = self.campaigns.set_status(&campaign_id, next) {
            tracing::error!(
                campaign_id = %campaign_id,
                %error,
                "failed to settle campaign status after worldgen"
            );
            return;
        }
        if finished {
            tracing::info!(campaign_id = %campaign_id, "world generation complete");
        } else {
            // The campaign is back in setup; tell the UI it may retry.
            self.fail(
                &campaign_id,
                AppError::Domain(
                    "world generation did not complete; you can try again"
                        .to_string(),
                ),
            );
        }
    }

    /// Lock-unwrapped view of the in-flight set (a poisoned lock degrades to
    /// an empty set, i.e. "nothing in flight" — safe, the guard just re-runs).
    fn intros_in_flight_set(
        &self,
    ) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.intros_in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Request cancellation of the running turn for a campaign.
    pub fn cancel_turn(&self, campaign_id: &str) {
        // Record the request; the loop checks this flag between iterations.
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
        // Delegated; ordering and the limit are enforced in SQL.
        repo::list_messages(self.storage.as_ref(), campaign_id, limit)
    }

    /// The agentic loop. Runs on a background task; never returns an error to a
    /// caller — failures are logged and surfaced as [`AppEvent::TurnError`].
    ///
    /// `user_message` is `None` for GM-initiated turns (setup intro, worldgen)
    /// where the system prompt alone instructs the GM to speak. Returns whether
    /// the turn ended cleanly (a narrative answer); callers use that to settle
    /// lifecycle state (worldgen → active vs. revert).
    async fn run_loop(
        &self,
        campaign_id: &str,
        user_message: Option<&MessageDto>,
        turn_index: i64,
        phase: TurnPhase,
        max_iterations: usize,
    ) -> bool {
        // Publish the player's message so the UI shows it immediately; GM-
        // initiated turns have none and skip this step.
        if let Some(message) = user_message {
            self.publish_turn_message(message);
        }

        // Build the initial provider conversation.
        let mut context =
            match self.build_context(campaign_id, user_message, phase).await {
                Ok(context) => context,
                Err(error) => {
                    // Context assembly failed (e.g. broken campaign/ruleset):
                    // surface a turn error and stop; nothing to complete.
                    self.fail(campaign_id, error);
                    return false;
                }
            };

        // `finished` records a clean end (narrative, no tool calls). If the
        // loop exits with it still false, it hit the cap or was cancelled.
        let mut finished = false;
        // The loop is bounded by `max_iterations`: a model stuck in a tool-call
        // cycle can only burn a fixed number of round-trips. Normal turns get
        // the tight default; worldgen gets its own larger budget.
        for iteration in 1..=max_iterations {
            // Check cancellation between iterations so an abort lands fast
            // instead of waiting for the next provider round-trip.
            if self.is_cancelled(campaign_id) {
                tracing::info!(campaign_id = %campaign_id, "turn cancelled");
                return false;
            }

            // One provider call with the current context; the response either
            // advances the story or requests tool executions.
            let response =
                match self.call_provider(&context, campaign_id, phase).await {
                    Ok(response) => response,
                    Err(error) => {
                        // Provider failure (timeout, bad key, outage): surface
                        // the typed error and stop the loop rather than retry.
                        self.fail(campaign_id, error);
                        return false;
                    }
                };

            // Ask the registry which model actually answered, so the row can
            // carry provenance; fall back to "unknown" if none is set.
            let model = self
                .providers
                .default_model()
                .unwrap_or_else(|| "unknown".to_string());
            let assistant = MessageDto::from_assistant(
                campaign_id,
                &response.message,
                &model,
                turn_index,
            );
            // Persist the GM row; a failure here must not crash the turn, so
            // it degrades to a warning and the loop continues (the UI event
            // below still fires from memory).
            repo::insert_message(self.storage.as_ref(), &assistant)
                .unwrap_or_else(|error| tracing::warn!(%error, "failed to persist assistant message"));
            // Stream the row to the UI as an event immediately after saving.
            self.publish_turn_message(&assistant);

            // Did the GM ask for tools?
            let tool_calls = response
                .message
                .content
                .iter()
                // Count only ToolCall blocks; text blocks don't request tools.
                .filter(|block| matches!(block, ContentBlock::ToolCall { .. }))
                .count();

            if tool_calls == 0 {
                // Narrative answer: the GM is done; push it and finish.
                context.push(response.message);
                finished = true;
                break;
            }

            // Clone: response.message is still needed below for the tool loop.
            context.push(response.message.clone());
            let tool_messages = self
                .execute_tool_calls(
                    campaign_id,
                    &response.message,
                    turn_index,
                    &model,
                )
                .await;
            match tool_messages {
                Ok(messages) => {
                    // Every tool result becomes a transcript row, an event,
                    // and a context entry so the model sees the outcomes.
                    for tool_message in &messages {
                        if let Err(error) = repo::insert_message(
                            self.storage.as_ref(),
                            tool_message,
                        ) {
                            // Persistence hiccup on a result row: warn and
                            // carry on rather than abort the whole turn.
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
                        // An inline synthetic result reports the error so the
                        // next provider call can react to the failure.
                        content: vec![ContentBlock::ToolResult {
                            id: new_id(),
                            result: json!({ "ok": false, "error": error.to_string() }),
                        }],
                    });
                }
            }
            // End of one tool iteration; the loop repeats, feeding results back
            // to the model until a narrative answer or the cap is reached.
            tracing::info!(campaign_id = %campaign_id, iteration, "tool iteration complete");
        }

        // Three exits: a narrative answer (success), a cancel request, or the
        // tool-iteration cap (degraded to an error event, never a crash). The
        // boolean is returned so callers (e.g. worldgen) can settle state.
        if finished {
            // The GM produced narrative prose: declare the turn complete.
            self.bus.publish(AppEvent::TurnComplete {
                campaign_id: campaign_id.to_string(),
                turn_index,
            });
            tracing::info!(campaign_id = %campaign_id, turn_index, "turn complete");
        } else if self.is_cancelled(campaign_id) {
            // Cancellation is a normal, quiet exit — no error event.
            tracing::info!(campaign_id = %campaign_id, "turn cancelled");
        } else {
            // Hit the iteration cap: degrade gracefully rather than erroring.
            self.bus.publish(AppEvent::TurnError {
                campaign_id: campaign_id.to_string(),
                message: "tool loop exceeded its iteration cap".to_string(),
            });
            tracing::warn!(campaign_id = %campaign_id, "tool iteration cap reached");
        }
        finished
    }

    /// Resolve the campaign's ruleset (or the built-in default).
    fn resolve_ruleset(&self, ruleset_id: Option<&str>) -> Result<Ruleset> {
        let ruleset = match ruleset_id {
            // An explicit binding must resolve to a real row, or the campaign
            // is misconfigured and the turn cannot proceed.
            Some(id) => self.rulesets.get(id)?.ok_or_else(|| {
                AppError::Domain(format!("ruleset not found: {id}"))
            })?,
            None => {
                // No explicit ruleset: fall back to the built-in default so an
                // unconfigured campaign still runs out of the box.
                let builtin = self
                    .rulesets
                    .list()?
                    .into_iter()
                    .find(|ruleset| ruleset.is_builtin);
                match builtin {
                    Some(ruleset) => ruleset,
                    None => {
                        // No built-in exists either (seed never ran): this is
                        // a broken install state, surfaced as a domain error.
                        return Err(AppError::Domain(
                            "no ruleset configured for campaign".to_string(),
                        ));
                    }
                }
            }
        };
        Ok(ruleset)
    }

    /// Build the full provider conversation: system, history, current action.
    ///
    /// `user_message` is `None` for GM-initiated turns; the system prompt must
    /// then itself instruct the GM to speak.
    async fn build_context(
        &self,
        campaign_id: &str,
        user_message: Option<&MessageDto>,
        phase: TurnPhase,
    ) -> Result<Vec<ChatMessage>> {
        // The campaign gates everything else: unknown id aborts early.
        let campaign = self.campaigns.get(campaign_id)?.ok_or_else(|| {
            AppError::Domain("campaign not found".to_string())
        })?;
        // Ruleset supplies the GM persona + instructions section.
        let ruleset = self.resolve_ruleset(campaign.ruleset_id.as_deref())?;
        // The current world document is presented as ground truth.
        let world_document = self.world.get_document(campaign_id)?;
        // The full roster of characters appears in the system prompt.
        let characters = self.characters.list_for_campaign(campaign_id)?;
        // The recent transcript window (oldest→newest) becomes the chat
        // history; the fixed window bounds every turn's token cost.
        let history = repo::recent_messages(
            self.storage.as_ref(),
            campaign_id,
            HISTORY_WINDOW,
        )?;

        let system = self.build_system_message(
            &ruleset,
            &world_document,
            &characters,
            phase,
        )?;
        // The player's persona name makes mode prefixes read naturally ("Elara
        // says: ..."); before a persona exists (setup chat) fall back to "You".
        let character_name = characters
            .iter()
            .find(|character| character.is_player)
            .map(|character| character.name.as_str())
            .unwrap_or("You");
        // Order matters: system, then history (oldest→newest), then the fresh
        // user action last so the model sees the current input most recently.
        let mut context = vec![system];
        for stored in history {
            // The freshly persisted user row is pushed again explicitly at the
            // end (below) so the current input is always the last message;
            // skipping it here prevents the same line from appearing twice in
            // the prompt with its mode prefix duplicated.
            if let Some(current) = user_message {
                if stored.id == current.id {
                    continue;
                }
            }
            // Rebuild each stored row as a chat message; the content blocks
            // carry over as-is (already provider-agnostic).
            context.push(to_chat_message(character_name, &stored));
        }
        // Append the fresh user action explicitly so the model sees the
        // current input as the last message — the most recently relevant
        // instruction, per the ordering contract above. GM-initiated turns
        // have no fresh input; their system prompt already speaks for itself.
        if let Some(current) = user_message {
            context.push(to_chat_message(character_name, current));
        }
        Ok(context)
    }

    /// Assemble the system prompt: a phase-specific opening, the world state,
    /// the character roster, and (outside setup) the tool list.
    fn build_system_message(
        &self,
        ruleset: &Ruleset,
        world_document: &serde_json::Value,
        characters: &[roleplayer_characters::domain::Character],
        phase: TurnPhase,
    ) -> Result<ChatMessage> {
        // The opening section differs by phase: setup runs the pre-world Q&A,
        // worldgen builds the world then opens the story, active plays normally.
        let mut sections = match phase {
            TurnPhase::Setup => vec![
                "You are the Game Master preparing a new roleplay. \
                 The world does not exist yet — you are building it together \
                 with the player. Introduce yourself warmly in your first \
                 message, then ask ONE clarifying question at a time about the \
                 setting, the tone, and the player's character. Do not narrate \
                 the story yet."
                    .to_string(),
            ],
            TurnPhase::Worldgen => vec![
                "You are the Game Master opening a new roleplay. The setup \
                 conversation above settled the premise; now build it. Call the \
                 tools to persist the world: use update_world for every \
                 important place, condition, and fact, and create_character for \
                 the player persona and the key NPCs. Batch as many tool calls \
                 into each response as you can. Then, in your final message, \
                 narrate a vivid opening scene that drops the player into the \
                 world."
                    .to_string(),
            ],
            TurnPhase::Active => vec![ruleset.system_prompt.clone()],
        };

        // World state is presented as ground truth; only tool calls change it.
        sections.push(format!(
            "\n## CURRENT WORLD STATE (this is the truth)\n{}",
            // Pretty-print for model readability; a serialize failure is
            // practically impossible for a JSON value, so fall back to `{}`.
            serde_json::to_string_pretty(world_document)
                .unwrap_or_else(|_| "{}".to_string())
        ));

        // Only add a CHARACTERS section when the roster is non-empty; an empty
        // campaign gets a shorter, clearer prompt.
        if !characters.is_empty() {
            // One bullet per character: name, player/NPC tag, and stats JSON.
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

        // In setup there is nothing to write yet, so no tools are advertised;
        // in worldgen and active the model gets the full tool set.
        if phase != TurnPhase::Setup {
            // List every registered command so the model knows what it can call.
            sections.push(
                "\n## AVAILABLE TOOLS (use them to change the world)"
                    .to_string(),
            );
            for command in &self.commands {
                sections.push(format!(
                    "- {}: {}",
                    command.id(),
                    command.description()
                ));
            }
            // Nudge the model toward tool use: without this reminder, models
            // tend to narrate state changes instead of calling tools, which
            // would make the world document diverge from the story.
            sections.push(
                "\nRemember: narrative free text never changes the world. Only tool calls do."
                    .to_string(),
            );
        }

        // Explain the two player input modes so the GM parses the prefixed
        // history correctly: `says: "..."` is dialogue, `acts: ...` narration.
        sections.push(
            "\nPlayer lines are prefixed with their mode: '<name> says: \"...\"' \
             is spoken dialogue (respond to it in character), '<name> acts: ...' \
             is a narrated action (respond to the deed)."
                .to_string(),
        );

        // Join the sections with blank lines into a single system message.
        Ok(ChatMessage::text(Role::System, sections.join("\n")))
    }

    /// Run one provider call (streamed when the provider supports it).
    async fn call_provider(
        &self,
        context: &[ChatMessage],
        campaign_id: &str,
        phase: TurnPhase,
    ) -> Result<roleplayer_core::llm::CompletionResponse> {
        // The default adapter from the registry; a missing default is a
        // configuration error, surfaced as such.
        let provider = self.providers.require_default()?;
        // Read capability flags once; every decision below degrades on them
        // (§5.5) so no provider is asked to do what it cannot.
        let capabilities = provider.capabilities();
        let model = self.providers.default_model().ok_or_else(|| {
            AppError::Config("no default model configured".to_string())
        })?;

        // Expose tools to the model only when the provider supports tool use
        // AND the phase allows writing (setup must not mutate a world that
        // does not exist yet).
        let tools: Vec<ToolSchema> =
            if phase != TurnPhase::Setup && capabilities.tool_use {
                self.commands.iter().map(|command| command.schema()).collect()
            } else {
                // Capability degradation (§5.5): no tool support, narrate only.
                vec![]
            };

        let request = CompletionRequest {
            model,
            // A snapshot of the context; the loop owns the mutable copy.
            messages: context.to_vec(),
            tools,
            // No temperature override: the GM's default sampling is fine.
            temperature: None,
            // Cap the output to what the provider advertises, so a runaway
            // model cannot emit unbounded text (§5.12, §5.17).
            max_tokens: Some(capabilities.max_output_tokens as u32),
            stream: capabilities.streaming,
        };

        // Stream when supported so the UI can render tokens live; the delta
        // callback fans them out as events. Otherwise fall back to one-shot
        // completion (capability degradation, §5.5).
        if capabilities.streaming {
            // Clone the bus and the id into the callback: the closure outlives
            // this call, so it must own everything it touches.
            let bus = self.bus.clone();
            let campaign_id = campaign_id.to_string();
            let on_delta = Box::new(move |delta: String| {
                // Each delta becomes a TurnDelta event the UI renders live.
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
        // Scan the message's blocks; only ToolCall blocks are actionable.
        for block in &assistant.content {
            // Non-tool blocks (narration) are skipped without comment.
            let ContentBlock::ToolCall { id, tool, arguments } = block else {
                continue;
            };

            // Announce the call to the UI before executing it.
            self.bus.publish(AppEvent::TurnToolCall {
                campaign_id: campaign_id.to_string(),
                tool: tool.clone(),
                arguments: arguments.clone(),
            });

            // Look up the command by its tool id; an unknown id is an error
            // the model must recover from, not a crash.
            let command = match self
                .commands
                .iter()
                .find(|command| command.id() == tool)
            {
                Some(command) => command,
                None => {
                    // Unknown tool: tell the model so it can recover on the
                    // next iteration rather than silently dropping the call.
                    tracing::warn!(campaign_id = %campaign_id, tool, "unknown tool call");
                    results.push(MessageDto {
                        id: new_id(),
                        campaign_id: campaign_id.to_string(),
                        role: Role::Tool,
                        // Tool rows carry the default mode; mode is a player-only concept.
                        mode: MessageMode::Action,
                        content: vec![ContentBlock::ToolResult {
                            id: id.clone(),
                            result: json!({ "ok": false, "error": "unknown tool" }),
                        }],
                        model: Some(model.to_string()),
                        turn_index,
                        created_at: now_rfc3339(),
                    });
                    // Move on to the next block; this call is answered.
                    continue;
                }
            };

            // Snapshot before executing so the command reads a stable world
            // while its mutations apply on top of that same snapshot.
            let world_snapshot = self.world.get_document(campaign_id)?;
            let mut context = CommandContext {
                campaign_id: campaign_id.to_string(),
                world: world_snapshot,
                // The command appends its mutations here; they are applied
                // and audited after a successful execution.
                mutations: Vec::new(),
            };

            match command.execute(arguments.clone(), &mut context) {
                Ok(result) => {
                    // Apply + audit every declared mutation (§4.6). Mutations
                    // route by kind: world keys go to world_state, character
                    // creations to the characters service — the two writers
                    // turnflow already owns.
                    self.apply_mutations(
                        campaign_id,
                        &context.mutations,
                        tool,
                        arguments,
                    );
                    self.bus.publish(AppEvent::TurnToolResult {
                        campaign_id: campaign_id.to_string(),
                        tool: tool.clone(),
                        ok: true,
                    });
                    // Wrap the successful result as a Tool message for the
                    // transcript and for feeding back into the context.
                    results.push(MessageDto {
                        id: new_id(),
                        campaign_id: campaign_id.to_string(),
                        role: Role::Tool,
                        // Tool rows carry the default mode; mode is a player-only concept.
                        mode: MessageMode::Action,
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
                    // Execution failed (e.g. bad dice expression): surface it
                    // to the UI and return the error to the model as a result,
                    // keeping the turn alive for a retry.
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
                        // Tool rows carry the default mode; mode is a player-only concept.
                        mode: MessageMode::Action,
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

    /// Apply a batch of declared mutations, routing each kind to its writer.
    ///
    /// `SetWorldKey` mutations go to world_state (which writes the audit trail
    /// — the anti-hallucination record). `CreateCharacter` mutations go to the
    /// characters service; a character row with its own `created_at` IS the
    /// record, so no `state_changes` audit row is written for it (that table
    /// tracks before/after world-document diffs). Failures are logged loudly
    /// but never crash the turn — the tool result already told the model it
    /// succeeded, so the loop keeps going.
    ///
    /// Public so integration tests can exercise the routing directly.
    pub fn apply_mutations(
        &self,
        campaign_id: &str,
        mutations: &[StateMutation],
        tool: &str,
        arguments: &serde_json::Value,
    ) {
        // Route world-key mutations as a batch to the audit trail.
        let world_mutations: Vec<StateMutation> = mutations
            .iter()
            .filter(|mutation| {
                matches!(mutation, StateMutation::SetWorldKey { .. })
            })
            .cloned()
            .collect();
        if !world_mutations.is_empty() {
            if let Err(error) = self.world.apply_mutations(
                campaign_id,
                &world_mutations,
                tool,
                arguments,
                // No transcript link: tool rows are linked via their own
                // message id downstream, not here.
                None,
            ) {
                tracing::error!(
                    campaign_id = %campaign_id,
                    tool,
                    %error,
                    "failed to apply world mutations"
                );
            }
        }
        // Route character creations individually to the characters service.
        for mutation in mutations {
            let StateMutation::CreateCharacter { name, bio, is_player, stats } =
                mutation
            else {
                continue;
            };
            match self.characters.create(NewCharacter {
                campaign_id: campaign_id.to_string(),
                name: name.clone(),
                bio: bio.clone(),
                is_player: *is_player,
                stats: stats.clone(),
            }) {
                Ok(character) => tracing::info!(
                    campaign_id = %campaign_id,
                    character_id = %character.id,
                    name = %character.name,
                    "GM created character"
                ),
                Err(error) => tracing::error!(
                    campaign_id = %campaign_id,
                    name = %name,
                    %error,
                    "failed to create character from GM tool call"
                ),
            }
        }
    }

    fn publish_turn_message(&self, message: &MessageDto) {
        // Serialize failure here is practically impossible (MessageDto is a
        // plain DTO); skip the event on the off chance rather than panic.
        if let Ok(value) = serde_json::to_value(message) {
            self.bus.publish(AppEvent::TurnMessage {
                campaign_id: message.campaign_id.clone(),
                message: value,
            });
        }
    }

    fn fail(&self, campaign_id: &str, error: AppError) {
        // Log full detail for diagnostics, then surface a summary event the
        // UI renders (§5.13, §5.15) — never a raw error across the bus.
        tracing::error!(campaign_id = %campaign_id, %error, "turn failed");
        self.bus.publish(AppEvent::TurnError {
            campaign_id: campaign_id.to_string(),
            message: error.to_string(),
        });
    }

    fn is_cancelled(&self, campaign_id: &str) -> bool {
        // A poisoned lock (a panic in another holder) degrades to "not
        // cancelled" — safe, and the turn simply continues.
        self.cancelled
            .lock()
            .map(|set| set.contains(campaign_id))
            .unwrap_or(false)
    }
}

/// Convert a stored transcript row into a provider chat message.
///
/// Player rows carry their action/speech `mode`; the mode is turned into a
/// prefix here (at prompt-build time, never stored) so the GM knows whether
/// the line is dialogue or narration. Other roles pass through unchanged.
fn to_chat_message(character_name: &str, message: &MessageDto) -> ChatMessage {
    // Mode is a player-only concept; only Role::User rows read it.
    if message.role != Role::User {
        // Assistant/tool/system rows keep their content block-for-block.
        return ChatMessage {
            role: message.role,
            content: message.content.clone(),
        };
    }
    // Concatenate the player's text blocks; user rows are prose-only, but
    // joining keeps the helper total even if a future block type appears.
    let text = message
        .content
        .iter()
        .filter_map(ContentBlock::text)
        .collect::<Vec<_>>()
        .join("\n");
    // Speech is dialogue (quoted), action is narration (unquoted) — the two
    // modes the composer offers map to these two shapes. The verb conjugates
    // for the "You" fallback ("You say", not "You says").
    let prefixed = match message.mode {
        MessageMode::Speech if character_name == "You" => {
            format!("You say: \"{text}\"")
        }
        MessageMode::Speech => format!("{character_name} says: \"{text}\""),
        MessageMode::Action if character_name == "You" => {
            format!("You act: {text}")
        }
        MessageMode::Action => format!("{character_name} acts: {text}"),
    };
    ChatMessage::text(Role::User, prefixed)
}

/// Convert a persisted tool-result row back into a provider chat message.
fn tool_message_to_chat(message: &MessageDto) -> ChatMessage {
    // Only the role + content matter to the provider; the DTO's id, turn
    // index, and timestamps are transcript concerns, not prompt concerns.
    ChatMessage { role: Role::Tool, content: message.content.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal user row; the mode is the only field each test varies.
    fn user_row(text: &str, mode: MessageMode) -> MessageDto {
        MessageDto {
            id: new_id(),
            campaign_id: "c1".to_string(),
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            mode,
            model: None,
            turn_index: 1,
            created_at: now_rfc3339(),
        }
    }

    #[test]
    fn speech_mode_quotes_the_line_for_the_gm() {
        // Dialogue must be wrapped in quotes so the GM reads it as spoken words.
        let chat = to_chat_message(
            "Elara",
            &user_row("I swear on my honor", MessageMode::Speech),
        );
        let text =
            chat.content.iter().find_map(ContentBlock::text).expect("text");
        assert_eq!(text, "Elara says: \"I swear on my honor\"");
    }

    #[test]
    fn action_mode_narrates_the_deed() {
        // Actions are narration, so the prefix reads as a deed, not a quote.
        let chat = to_chat_message(
            "Elara",
            &user_row("draw my sword", MessageMode::Action),
        );
        let text =
            chat.content.iter().find_map(ContentBlock::text).expect("text");
        assert_eq!(text, "Elara acts: draw my sword");
    }

    #[test]
    fn non_user_rows_pass_through_unchanged() {
        // The GM's own rows must reach the provider exactly as stored — the
        // mode prefix is a player-message concern only.
        let assistant = MessageDto {
            id: new_id(),
            campaign_id: "c1".to_string(),
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: "The door creaks.".to_string(),
            }],
            mode: MessageMode::Action,
            model: Some("mock".to_string()),
            turn_index: 1,
            created_at: now_rfc3339(),
        };
        let chat = to_chat_message("Elara", &assistant);
        assert_eq!(chat.role, Role::Assistant);
        assert_eq!(chat.content[0].text(), Some("The door creaks."));
    }
}
