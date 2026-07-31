//! Thin Tauri IPC commands for the `characters` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::{Character, NewCharacter, UpdateCharacter};
use crate::service::CharacterService;

type SharedCharacterService = Arc<CharacterService<Database>>;

/// Command: list all characters of a campaign.
#[tauri::command]
pub fn list_characters(
    service: State<'_, SharedCharacterService>,
    campaign_id: String,
) -> Result<Vec<Character>, ErrorDto> {
    service.list_for_campaign(&campaign_id).map_err(ErrorDto::from)
}

/// Command: create a character.
#[tauri::command]
pub fn create_character(
    service: State<'_, SharedCharacterService>,
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
    service.get(&character_id).map_err(ErrorDto::from)
}

/// Command: update a character's editable fields.
#[tauri::command]
pub fn update_character(
    service: State<'_, SharedCharacterService>,
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
    service.delete(&character_id).map_err(ErrorDto::from)
}
