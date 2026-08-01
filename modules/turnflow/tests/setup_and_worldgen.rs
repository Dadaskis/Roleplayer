//! Setup → worldgen → active lifecycle integration test (§5.11, headless).
//!
//! Exercises the GM-initiated turn primitives with the Mock provider: the
//! setup intro opens the session, its idempotency guard rejects a second kick,
//! start_roleplay flips the campaign to worldgen, and a completed worldgen
//! turn settles the campaign as active.

use std::sync::Arc;
use std::time::Duration;

use roleplayer_campaigns::domain::{CampaignStatus, NewCampaign};
use roleplayer_campaigns::service::CampaignService;
use roleplayer_characters::service::CharacterService;
use roleplayer_core::eventbus::{AppEvent, EventBus};
use roleplayer_core::game_command::StateMutation;
use roleplayer_core::llm::Role;
use roleplayer_core::storage::Database;
use roleplayer_providers::registry::ProviderRegistry;
use roleplayer_providers::service::ProviderService;
use roleplayer_rulesets::service::RulesetService;
use roleplayer_turnflow::service::TurnService;
use roleplayer_world_state::service::WorldStateService;

/// The service stack the app crate wires, with the seams surfaced so tests can
/// subscribe for turn events and read the aggregate back.
struct Stack {
    turnflow: Arc<TurnService<Database>>,
    campaigns: Arc<CampaignService<Database>>,
    characters: Arc<CharacterService<Database>>,
    bus: EventBus,
}

/// Build the same service stack the app crate wires (mock as the default).
fn build_stack() -> Stack {
    let storage = Arc::new(Database::open_in_memory().expect("in-memory db"));
    let bus = EventBus::new();

    let campaigns = Arc::new(CampaignService::new(storage.clone()));
    let rulesets = Arc::new(RulesetService::new(storage.clone()));
    rulesets.ensure_default().expect("seed ruleset");
    let world = Arc::new(WorldStateService::new(storage.clone()));
    let characters = Arc::new(CharacterService::new(storage.clone()));

    let registry = Arc::new(ProviderRegistry::new());
    let providers =
        Arc::new(ProviderService::new(storage.clone(), registry.clone()));
    providers.ensure_defaults().expect("seed providers");
    providers.set_default("mock").expect("default mock");

    let turnflow = Arc::new(TurnService::new(
        storage.clone(),
        world,
        campaigns.clone(),
        characters.clone(),
        rulesets,
        registry,
        bus.clone(),
    ));
    Stack { turnflow, campaigns, characters, bus }
}

/// Wait for a TurnComplete on the bus (10s guard against a hung loop).
async fn wait_for_completion(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    campaign_id: &str,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining =
            deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .expect("timed out waiting for a turn event");
        match event {
            Ok(AppEvent::TurnComplete { campaign_id: done, .. })
                if done == campaign_id =>
            {
                break;
            }
            // Fail loudly on errors: the stack should never error here.
            Ok(AppEvent::TurnError { message, .. }) => {
                panic!("turn failed: {message}")
            }
            _ => continue,
        }
    }
}

#[tokio::test]
async fn setup_intro_is_idempotent_and_opens_the_session() {
    let stack = build_stack();
    let campaign = stack
        .campaigns
        .create(NewCampaign {
            name: "Setup Test".to_string(),
            description: String::new(),
            ruleset_id: None,
        })
        .expect("create campaign");
    // Subscribe before the intro runs so no event slips past.
    let mut events = stack.bus.subscribe();

    // A fresh setup campaign with no messages is due for an intro.
    let started =
        stack.turnflow.start_setup_intro(&campaign.id).expect("start intro");
    assert!(started, "a fresh setup campaign is due for an intro");

    // A second kick while the first is in flight must be refused (the guard).
    let second =
        stack.turnflow.start_setup_intro(&campaign.id).expect("second start");
    assert!(!second, "the in-flight guard must reject a second intro");

    // Run the intro to completion; clone the id so the spawned task and the
    // wait loop below both hold it (the async move would otherwise steal it).
    let campaign_id = campaign.id.clone();
    let runner = stack.turnflow.clone();
    let _handle = tokio::spawn(async move {
        runner.run_setup_intro(campaign_id.clone()).await;
    });
    wait_for_completion(&mut events, &campaign.id).await;

    // The GM's opening message persisted into the transcript.
    let messages =
        stack.turnflow.list_messages(&campaign.id, 10).expect("list messages");
    assert!(!messages.is_empty(), "the intro must produce a GM message");
    assert_eq!(
        messages[0].role,
        Role::Assistant,
        "the first message is the GM's"
    );
    // The transcript now has rows, so a later intro kick is a no-op.
    let later = stack
        .turnflow
        .start_setup_intro(&campaign.id)
        .expect("start after intro");
    assert!(!later, "a campaign with a transcript is not due for an intro");
}

#[tokio::test]
async fn start_roleplay_generates_then_activates_the_campaign() {
    let stack = build_stack();
    let campaign = stack
        .campaigns
        .create(NewCampaign {
            name: "Worldgen Test".to_string(),
            description: String::new(),
            ruleset_id: None,
        })
        .expect("create campaign");
    let mut events = stack.bus.subscribe();

    // start_roleplay flips setup → worldgen immediately.
    stack.turnflow.start_roleplay(&campaign.id).expect("start roleplay");
    let status =
        stack.campaigns.get(&campaign.id).expect("get").expect("exists").status;
    assert_eq!(status, CampaignStatus::Worldgen, "generation is single-flight");

    // A second start while worldgen is in flight must refuse.
    let second = stack.turnflow.start_roleplay(&campaign.id);
    assert!(
        second.is_err(),
        "worldgen is single-flight; a double start is refused"
    );

    // Run the generation turn; the Mock narrates an opening, which counts as
    // a clean finish → the campaign settles as active.
    let campaign_id = campaign.id.clone();
    let runner = stack.turnflow.clone();
    let _handle = tokio::spawn(async move {
        runner.run_worldgen(campaign_id.clone()).await;
    });
    wait_for_completion(&mut events, &campaign.id).await;

    let settled =
        stack.campaigns.get(&campaign.id).expect("get").expect("exists").status;
    assert_eq!(
        settled,
        CampaignStatus::Active,
        "a completed worldgen activates"
    );
    // The generation turn produced at least one GM message.
    let messages =
        stack.turnflow.list_messages(&campaign.id, 10).expect("list messages");
    assert!(!messages.is_empty(), "worldgen must produce an opening message");
}

#[tokio::test]
async fn apply_mutations_routes_character_creation_to_the_roster() {
    // The cross-module integration point the Mock-driven turns never reach:
    // a CreateCharacter mutation must land as a real character row.
    let stack = build_stack();
    let campaign = stack
        .campaigns
        .create(NewCampaign {
            name: "Routing Test".to_string(),
            description: String::new(),
            ruleset_id: None,
        })
        .expect("create campaign");

    // Call the same routing helper the tool loop uses, with a single
    // character-creation mutation.
    stack.turnflow.apply_mutations(
        &campaign.id,
        &[StateMutation::CreateCharacter {
            name: "Elara".to_string(),
            bio: "A ranger of the Duskmoor".to_string(),
            is_player: true,
            stats: serde_json::json!({ "hp": 12 }),
        }],
        "create_character",
        &serde_json::json!({ "name": "Elara" }),
    );

    // The character now exists, scoped to the campaign, with its fields intact.
    let roster =
        stack.characters.list_for_campaign(&campaign.id).expect("list roster");
    assert_eq!(
        roster.len(),
        1,
        "the mutation must create exactly one character"
    );
    assert_eq!(roster[0].name, "Elara");
    assert!(roster[0].is_player);
    assert_eq!(roster[0].stats["hp"], 12);
}
