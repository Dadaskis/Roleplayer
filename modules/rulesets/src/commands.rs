//! Thin Tauri IPC commands for the `rulesets` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::{NewRuleset, Ruleset, UpdateRuleset};
use crate::service::RulesetService;

type SharedRulesetService = Arc<RulesetService<Database>>;

/// Command: list all rulesets (built-ins first).
#[tauri::command]
pub fn list_rulesets(
    service: State<'_, SharedRulesetService>,
) -> Result<Vec<Ruleset>, ErrorDto> {
    service.list().map_err(ErrorDto::from)
}

/// Command: fetch one ruleset by id.
#[tauri::command]
pub fn get_ruleset(
    service: State<'_, SharedRulesetService>,
    ruleset_id: String,
) -> Result<Option<Ruleset>, ErrorDto> {
    service.get(&ruleset_id).map_err(ErrorDto::from)
}

/// Command: create a custom ruleset.
#[tauri::command]
pub fn create_ruleset(
    service: State<'_, SharedRulesetService>,
    input: NewRuleset,
) -> Result<Ruleset, ErrorDto> {
    service.create(input).map_err(ErrorDto::from)
}

/// Command: update a custom ruleset.
#[tauri::command]
pub fn update_ruleset(
    service: State<'_, SharedRulesetService>,
    ruleset_id: String,
    input: UpdateRuleset,
) -> Result<Option<Ruleset>, ErrorDto> {
    service.update(&ruleset_id, input).map_err(ErrorDto::from)
}

/// Command: delete a custom ruleset.
#[tauri::command]
pub fn delete_ruleset(
    service: State<'_, SharedRulesetService>,
    ruleset_id: String,
) -> Result<bool, ErrorDto> {
    service.delete(&ruleset_id).map_err(ErrorDto::from)
}
