//! Thin Tauri IPC commands for the `turnflow` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::llm::MessageMode;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::service::{MessageDto, TurnService};

// Long-lived shared instance, injected via Tauri State; Arc for concurrency.
// `Database` is the concrete backend the app wires at startup.
type SharedTurnService = Arc<TurnService<Database>>;

/// Command: start an agentic turn. Returns the turn index; progress arrives as
/// `turn-event` events on the event bus (see the app crate).
#[tauri::command]
pub fn send_turn(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
    text: String,
    // The player's input mode ("action" or "speech"); validated at the
    // boundary so a hostile value can never reach storage (§5.10, §5.18).
    mode: String,
) -> Result<i64, ErrorDto> {
    // Parse the wire string; anything other than "speech" degrades to the
    // action default, matching the tolerant parsers used elsewhere.
    let mode = MessageMode::from_wire(&mode);
    // Clone the Arc so the spawned task can own the service independently of
    // the borrowed `State` guard, which only lives for this command call.
    let service = service.inner().clone();
    // Phase 1: validate + persist the user message synchronously, on this
    // thread (no runtime needed); errors reach the caller directly.
    let prepared = service
        .prepare_turn(&campaign_id, &text, mode)
        .map_err(ErrorDto::from)?;
    // Remember the index to return to the caller before execution starts.
    let turn_index = prepared.turn_index;

    // Run the loop on tauri's own async runtime. `tauri::async_runtime::spawn`
    // lazily ensures a runtime exists, so this is safe from a sync command
    // thread — a bare `tokio::spawn` here would panic ("no reactor running")
    // because sync commands run with no Tokio context.
    tauri::async_runtime::spawn(async move {
        // Phase 2: the agentic loop streams TurnMessage/TurnDelta events on
        // the bus; this future resolves when the turn ends.
        service.run_prepared(prepared).await;
    });

    // Return immediately with the reserved index; the UI matches events to
    // this turn by it.
    Ok(turn_index)
}

/// Command: kick off the setup-intro turn (the GM opens the session itself).
/// Returns whether a turn was actually started; the guard makes repeated calls
/// (StrictMode double-mount, double-clicks) safe no-ops.
#[tauri::command]
pub fn start_setup_intro(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
) -> Result<bool, ErrorDto> {
    // The idempotency check lives in the service (under the single writer),
    // not here — commands stay thin (§5.2).
    let started =
        service.start_setup_intro(&campaign_id).map_err(ErrorDto::from)?;
    if started {
        // The intro runs in the background on tauri's runtime.
        let service = service.inner().clone();
        tauri::async_runtime::spawn(async move {
            service.run_setup_intro(campaign_id).await;
        });
    }
    Ok(started)
}

/// Command: start the roleplay — the GM generates the world + characters and
/// opens the story. Returns once the generation flow has started; completion
/// arrives as turn events and the campaign's status change.
#[tauri::command]
pub fn start_roleplay(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
) -> Result<(), ErrorDto> {
    // Validate + flip setup → worldgen synchronously; the guard rejects a
    // double-invocation (a second click sees Worldgen and refuses).
    service.start_roleplay(&campaign_id).map_err(ErrorDto::from)?;
    // Run the worldgen turn on tauri's runtime; it settles the status itself.
    let service = service.inner().clone();
    tauri::async_runtime::spawn(async move {
        service.run_worldgen(campaign_id).await;
    });
    Ok(())
}

/// Command: cancel the running turn for a campaign.
#[tauri::command]
pub fn cancel_turn(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
) -> Result<(), ErrorDto> {
    // Fire-and-forget: the flag is picked up by the loop between iterations.
    service.cancel_turn(&campaign_id);
    Ok(())
}

/// Command: recent transcript rows for a campaign (oldest-first).
#[tauri::command]
pub fn list_messages(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
    limit: i64,
) -> Result<Vec<MessageDto>, ErrorDto> {
    service.list_messages(&campaign_id, limit).map_err(ErrorDto::from)
}
