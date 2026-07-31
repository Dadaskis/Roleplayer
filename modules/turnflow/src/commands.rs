//! Thin Tauri IPC commands for the `turnflow` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::service::{MessageDto, TurnService};

type SharedTurnService = Arc<TurnService<Database>>;

/// Command: start an agentic turn. Returns the turn index; progress arrives as
/// `turn-event` events on the event bus (see the app crate).
#[tauri::command]
pub fn send_turn(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
    text: String,
) -> Result<i64, ErrorDto> {
    let service = service.inner().clone();
    let prepared =
        service.prepare_turn(&campaign_id, &text).map_err(ErrorDto::from)?;
    let turn_index = prepared.turn_index;

    // Run the loop on tauri's own async runtime. `tauri::async_runtime::spawn`
    // lazily ensures a runtime exists, so this is safe from a sync command
    // thread — a bare `tokio::spawn` here would panic ("no reactor running")
    // because sync commands run with no Tokio context.
    tauri::async_runtime::spawn(async move {
        service.run_prepared(prepared).await;
    });

    Ok(turn_index)
}

/// Command: cancel the running turn for a campaign.
#[tauri::command]
pub fn cancel_turn(
    service: State<'_, SharedTurnService>,
    campaign_id: String,
) -> Result<(), ErrorDto> {
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
