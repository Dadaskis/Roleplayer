//! Thin Tauri IPC commands for the `search` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::SearchResult;
use crate::service::SearchService;

type SharedSearchService = Arc<SearchService<Database>>;

/// Command: search a campaign's transcript.
#[tauri::command]
pub fn search_messages(
    service: State<'_, SharedSearchService>,
    campaign_id: String,
    query: String,
    limit: i64,
) -> Result<Vec<SearchResult>, ErrorDto> {
    service.search(&campaign_id, &query, limit).map_err(ErrorDto::from)
}
