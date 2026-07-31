//! Real-provider smoke test: a full agentic turn against `opencode-go` when
//! `OPENCODE_API_KEY` is available (§5.11, §5.17, §7 of AGENTS.md).
//!
//! Skips (passes quietly) without the key, so CI never depends on a live API.
//! With the key it proves the whole stack — provider adapter, agentic loop,
//! transcript persistence, events — against a real model.

use std::sync::Arc;
use std::time::Duration;

use roleplayer_campaigns::domain::NewCampaign;
use roleplayer_campaigns::service::CampaignService;
use roleplayer_core::eventbus::{AppEvent, EventBus};
use roleplayer_core::llm::Role;
use roleplayer_core::storage::Database;
use roleplayer_providers::registry::ProviderRegistry;
use roleplayer_providers::service::ProviderService;
use roleplayer_rulesets::service::RulesetService;
use roleplayer_turnflow::service::TurnService;
use roleplayer_world_state::service::WorldStateService;

#[tokio::test]
async fn real_opencode_go_turn_completes() {
    // Without a key this is a graceful skip, not a failure.
    // An unset, empty, or whitespace-only variable all count as "no key",
    // so CI environments with no secret pass quietly.
    if std::env::var("OPENCODE_API_KEY")
        .map(|key| key.trim().is_empty())
        .unwrap_or(true)
    {
        eprintln!("skipping real-provider test: OPENCODE_API_KEY not set");
        return;
    }

    // In-memory storage + the real bus, same as the mock test: the only
    // difference is the provider actually talks to the network.
    let storage = Arc::new(Database::open_in_memory().expect("in-memory db"));
    let bus = EventBus::new();
    let mut events = bus.subscribe();

    let campaigns = Arc::new(CampaignService::new(storage.clone()));
    let campaign = campaigns
        .create(NewCampaign {
            name: "Real Provider Smoke".to_string(),
            description: String::new(),
            // Built-in ruleset fallback, exactly like a fresh user install.
            ruleset_id: None,
        })
        .expect("create campaign");

    let rulesets = Arc::new(RulesetService::new(storage.clone()));
    rulesets.ensure_default().expect("seed ruleset");

    let world = Arc::new(WorldStateService::new(storage.clone()));
    let registry = Arc::new(ProviderRegistry::new());
    let providers =
        Arc::new(ProviderService::new(storage.clone(), registry.clone()));
    providers.ensure_defaults().expect("seed providers");
    // ensure_defaults prefers opencode-go when the key exists — confirm it.
    let default_id = registry
        .get_default()
        .map(|provider| provider.id().to_string())
        .expect("a default provider");
    // If this assertion fires, the key-selection logic regressed: with a key
    // present the default must be the real provider, never the mock.
    assert_eq!(
        default_id, "opencode-go",
        "default must be the real provider here"
    );

    let turnflow = Arc::new(TurnService::new(
        storage.clone(),
        world.clone(),
        campaigns.clone(),
        Arc::new(roleplayer_characters::service::CharacterService::new(
            storage.clone(),
        )),
        rulesets.clone(),
        registry.clone(),
        bus.clone(),
    ));

    // The prompt invites a tool call but doesn't require one: the assertion
    // below only demands narrative prose, whatever the model chose to do.
    let prepared = turnflow
        .prepare_turn(
            &campaign.id,
            "A tavern door creaks open. I step inside and look around — describe the scene and set a fact about the room with your tools if it fits.",
        )
        .expect("prepare turn");
    let runner = turnflow.clone();
    let _handle = tokio::spawn(async move {
        runner.run_prepared(prepared).await;
    });

    // The real model may take a while; be generous.
    // Unlike the mock test, failures surface as TurnError events; any such
    // event means the stack misbehaved with a real provider, so fail loudly.
    let mut completed = false;
    loop {
        match tokio::time::timeout(Duration::from_secs(180), events.recv())
            .await
        {
            Ok(Ok(AppEvent::TurnComplete { .. })) => {
                completed = true;
                break;
            }
            // A TurnError here is the strongest signal: a real provider made
            // the loop fail, so the test must not pass quietly.
            Ok(Ok(AppEvent::TurnError { message, .. })) => {
                panic!("real provider turn failed: {message}");
            }
            Ok(Ok(_other)) => continue,
            Ok(Err(_)) => break,
            Err(_elapsed) => break,
        }
    }
    assert!(completed, "real provider turn never completed (timeout)");

    // Persistence proof: at minimum the user action and one GM reply exist.
    let messages =
        turnflow.list_messages(&campaign.id, 100).expect("list messages");
    assert!(messages.len() >= 2, "expected at least a user + GM message");
    // Collect every text block from assistant rows into one narrative string.
    let gm_text: String = messages
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .flat_map(|message| {
            message.content.iter().filter_map(|block| match block {
                roleplayer_core::llm::ContentBlock::Text { text } => {
                    Some(text.clone())
                }
                _ => None,
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    // A real model could reply with only tool calls; this guards that the
    // final message still carries actual narrative prose for the user.
    assert!(
        !gm_text.trim().is_empty(),
        "GM produced no narrative text in a real turn"
    );
    // A short success line so a human running the test sees what happened.
    eprintln!(
        "real provider turn OK — GM replied with {} chars",
        gm_text.len()
    );
}
