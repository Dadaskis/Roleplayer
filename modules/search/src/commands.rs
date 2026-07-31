//! Thin Tauri IPC commands for the `search` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::SearchResult;
use crate::service::SearchService;

// Long-lived shared instance, injected via Tauri State; Arc for concurrency.
// `Database` is the concrete backend the app wires at startup.
type SharedSearchService = Arc<SearchService<Database>>;

/// Command: search a campaign's transcript.
#[tauri::command]
pub fn search_messages(
    service: State<'_, SharedSearchService>,
    campaign_id: String,
    // The raw query string from the UI; the service clamps the limit and the
    // repo binds the query as a parameter (never interpolated into SQL).
    query: String,
    limit: i64,
) -> Result<Vec<SearchResult>, ErrorDto> {
    service.search(&campaign_id, &query, limit).map_err(ErrorDto::from)
}
