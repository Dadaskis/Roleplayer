//! Thin Tauri IPC commands for the `characters` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::{Character, NewCharacter, UpdateCharacter};
use crate::service::CharacterService;

// Long-lived shared instance, injected via Tauri State; Arc for concurrency.
// `Database` is the concrete backend the app wires at startup.
type SharedCharacterService = Arc<CharacterService<Database>>;

/// Command: list all characters of a campaign.
#[tauri::command]
pub fn list_characters(
    // Tauri injects the shared service from managed state for this call.
    service: State<'_, SharedCharacterService>,
    campaign_id: String,
) -> Result<Vec<Character>, ErrorDto> {
    // Delegation only; the campaign scoping lives in the repo's SQL.
    service.list_for_campaign(&campaign_id).map_err(ErrorDto::from)
}

/// Command: create a character.
#[tauri::command]
pub fn create_character(
    service: State<'_, SharedCharacterService>,
    // Input validated inside the service (NewCharacter::validate), not here.
    input: NewCharacter,
) -> Result<Character, ErrorDto> {
    service.create(input).map_err(ErrorDto::from)
}

/// Command: fetch one character by id.
#[tauri::command]
pub fn get_character(
    service: State<'_, SharedCharacterService>,
    character_id: String,
) -> Result<Option<Character>, ErrorDto> {
    // None (unknown id) becomes JSON null on the wire, not an error.
    service.get(&character_id).map_err(ErrorDto::from)
}

/// Command: update a character's editable fields.
#[tauri::command]
pub fn update_character(
    service: State<'_, SharedCharacterService>,
    // The id is a lookup key only; the authoritative row comes from the DB.
    character_id: String,
    input: UpdateCharacter,
) -> Result<Option<Character>, ErrorDto> {
    service.update(&character_id, input).map_err(ErrorDto::from)
}

/// Command: delete a character.
#[tauri::command]
pub fn delete_character(
    service: State<'_, SharedCharacterService>,
    character_id: String,
) -> Result<bool, ErrorDto> {
    // The boolean tells the UI whether the character existed at all.
    service.delete(&character_id).map_err(ErrorDto::from)
}
