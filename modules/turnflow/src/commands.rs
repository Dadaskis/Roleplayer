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
    // Clone the inner Arc so the background loop can outlive the command.
    service.inner().clone().send_turn(campaign_id, text).map_err(ErrorDto::from)
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
