//! Thin Tauri IPC commands for the `memories` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::domain::{Memory, NewMemory};
use crate::service::MemoryService;

type SharedMemoryService = Arc<MemoryService<Database>>;

/// Wire shape for a summarization request.
#[derive(Debug, Serialize, Deserialize)]
pub struct SummarizeRequest {
    pub campaign_id: String,
    pub source_from: i64,
    pub source_to: i64,
}

/// Command: list memories of a campaign.
#[tauri::command]
pub fn list_memories(
    service: State<'_, SharedMemoryService>,
    campaign_id: String,
) -> Result<Vec<Memory>, ErrorDto> {
    service.list_for_campaign(&campaign_id).map_err(ErrorDto::from)
}

/// Command: create a memory manually.
#[tauri::command]
pub fn create_memory(
    service: State<'_, SharedMemoryService>,
    input: NewMemory,
) -> Result<Memory, ErrorDto> {
    service.create(input).map_err(ErrorDto::from)
}

/// Command: delete a memory.
#[tauri::command]
pub fn delete_memory(
    service: State<'_, SharedMemoryService>,
    memory_id: String,
) -> Result<bool, ErrorDto> {
    service.delete(&memory_id).map_err(ErrorDto::from)
}

/// Command: generate a summary of a turn range with the default provider.
#[tauri::command]
pub async fn summarize_memory(
    service: State<'_, SharedMemoryService>,
    input: SummarizeRequest,
) -> Result<Memory, ErrorDto> {
    service
        .generate_summary(
            &input.campaign_id,
            input.source_from,
            input.source_to,
        )
        .await
        .map_err(ErrorDto::from)
}
