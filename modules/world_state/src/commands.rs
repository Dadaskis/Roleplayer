//! Thin Tauri IPC commands for the `world_state` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use serde_json::Value;
use tauri::State;

use crate::domain::StateChange;
use crate::service::WorldStateService;

type SharedWorldStateService = Arc<WorldStateService<Database>>;

/// Command: read the current world document for a campaign.
#[tauri::command]
pub fn get_world_state(
    service: State<'_, SharedWorldStateService>,
    campaign_id: String,
) -> Result<Value, ErrorDto> {
    service.get_document(&campaign_id).map_err(ErrorDto::from)
}

/// Command: manually set one world key (records a "manual" audit entry).
#[tauri::command]
pub fn set_world_key(
    service: State<'_, SharedWorldStateService>,
    campaign_id: String,
    key: String,
    value: Value,
) -> Result<(Value, Value), ErrorDto> {
    service.set_key_manual(&campaign_id, &key, value).map_err(ErrorDto::from)
}

/// Command: manually remove one world key.
#[tauri::command]
pub fn remove_world_key(
    service: State<'_, SharedWorldStateService>,
    campaign_id: String,
    key: String,
) -> Result<(Value, Value), ErrorDto> {
    service.remove_key_manual(&campaign_id, &key).map_err(ErrorDto::from)
}

/// Command: list recent state-change audit entries for a campaign.
#[tauri::command]
pub fn list_state_changes(
    service: State<'_, SharedWorldStateService>,
    campaign_id: String,
    limit: i64,
) -> Result<Vec<StateChange>, ErrorDto> {
    service.list_changes(&campaign_id, limit).map_err(ErrorDto::from)
}
