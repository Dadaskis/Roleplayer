//! Thin Tauri IPC commands for the `rulesets` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::{NewRuleset, Ruleset, UpdateRuleset};
use crate::service::RulesetService;

// Long-lived shared instance, injected via Tauri State; Arc for concurrency.
// `Database` is the concrete backend the app wires at startup.
type SharedRulesetService = Arc<RulesetService<Database>>;

/// Command: list all rulesets (built-ins first).
#[tauri::command]
pub fn list_rulesets(
    service: State<'_, SharedRulesetService>,
) -> Result<Vec<Ruleset>, ErrorDto> {
    // Delegation only; "built-ins first" ordering lives in the repo's SQL.
    service.list().map_err(ErrorDto::from)
}

/// Command: fetch one ruleset by id.
#[tauri::command]
pub fn get_ruleset(
    service: State<'_, SharedRulesetService>,
    ruleset_id: String,
) -> Result<Option<Ruleset>, ErrorDto> {
    // None (unknown id) becomes JSON null on the wire, not an error.
    service.get(&ruleset_id).map_err(ErrorDto::from)
}

/// Command: create a custom ruleset.
#[tauri::command]
pub fn create_ruleset(
    service: State<'_, SharedRulesetService>,
    // Input validated inside the service (NewRuleset::validate), not here.
    input: NewRuleset,
) -> Result<Ruleset, ErrorDto> {
    service.create(input).map_err(ErrorDto::from)
}

/// Command: update a custom ruleset.
#[tauri::command]
pub fn update_ruleset(
    service: State<'_, SharedRulesetService>,
    // The id is a lookup key; the authoritative row comes from the DB.
    ruleset_id: String,
    input: UpdateRuleset,
) -> Result<Option<Ruleset>, ErrorDto> {
    // A None result also covers a refused edit of the immutable built-in.
    service.update(&ruleset_id, input).map_err(ErrorDto::from)
}

/// Command: delete a custom ruleset.
#[tauri::command]
pub fn delete_ruleset(
    service: State<'_, SharedRulesetService>,
    ruleset_id: String,
) -> Result<bool, ErrorDto> {
    // The boolean tells the UI whether a ruleset was actually removed.
    service.delete(&ruleset_id).map_err(ErrorDto::from)
}
