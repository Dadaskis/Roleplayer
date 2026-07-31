//! Full-turn integration test: a user action flows through the whole agentic
//! loop with the Mock provider — transcript persisted, tool call executed,
//! completion event emitted (§5.11: headless, no webview).

use std::sync::Arc;
use std::time::Duration;

use roleplayer_campaigns::domain::NewCampaign;
use roleplayer_campaigns::service::CampaignService;
use roleplayer_core::eventbus::AppEvent;
use roleplayer_core::eventbus::EventBus;
use roleplayer_core::llm::Role;
use roleplayer_core::storage::Database;
use roleplayer_providers::registry::ProviderRegistry;
use roleplayer_providers::service::ProviderService;
use roleplayer_rulesets::service::RulesetService;
use roleplayer_turnflow::service::TurnService;
use roleplayer_world_state::service::WorldStateService;

#[tokio::test]
async fn full_turn_with_mock_provider_completes() {
    // In-memory storage keeps the test headless and isolated.
    let storage = Arc::new(Database::open_in_memory().expect("in-memory db"));
    let bus = EventBus::new();
    let mut events = bus.subscribe();

    // Seed the aggregate: campaign + builtin ruleset + a player character.
    let campaigns = Arc::new(CampaignService::new(storage.clone()));
    let campaign = campaigns
        .create(NewCampaign {
            name: "Dungeon Test".to_string(),
            description: String::new(),
            ruleset_id: None,
        })
        .expect("create campaign");

    let rulesets = Arc::new(RulesetService::new(storage.clone()));
    rulesets.ensure_default().expect("seed ruleset");

    let world = Arc::new(WorldStateService::new(storage.clone()));

    // Providers: mock only, forced as the default regardless of env.
    let registry = Arc::new(ProviderRegistry::new());
    let providers =
        Arc::new(ProviderService::new(storage.clone(), registry.clone()));
    providers.ensure_defaults().expect("seed providers");
    providers.set_default("mock").expect("default mock");

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

    // Send a turn that triggers the Mock's dice tool call.
    let turn_index = turnflow
        .clone()
        .send_turn(
            campaign.id.clone(),
            "I swing my axe at the goblin — roll dice".to_string(),
        )
        .expect("send turn");
    assert_eq!(turn_index, 1, "first turn has index 1");

    // Wait for the TurnComplete event (10s guard against a hung loop).
    let mut completed = false;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await
        {
            Ok(Ok(AppEvent::TurnComplete {
                campaign_id,
                turn_index: completed_index,
            })) => {
                assert_eq!(campaign_id, campaign.id);
                assert_eq!(completed_index, 1);
                completed = true;
                break;
            }
            Ok(Ok(_other)) => continue,
            Ok(Err(_)) => break,
            Err(_elapsed) => break,
        }
    }
    assert!(completed, "turn never completed");

    // The transcript must contain the user action, a tool call, a tool result,
    // and a final assistant narrative.
    let messages =
        turnflow.list_messages(&campaign.id, 100).expect("list messages");
    assert!(messages.len() >= 4, "expected >=4 rows, got {}", messages.len());
    assert!(messages.iter().any(|message| message.role == Role::User));
    assert!(messages.iter().any(|message| message.role == Role::Assistant));
    assert!(messages.iter().any(|message| message.role == Role::Tool));

    // The world document was never touched by the dice path.
    let document = world.get_document(&campaign.id).expect("world doc");
    assert!(document
        .as_object()
        .map(|object| object.is_empty())
        .unwrap_or(false));
}
