//! Thin Tauri IPC commands for the `campaigns` module.
//!
//! These are pure pass-throughs (§5.2 of AGENTS.md): validate nothing beyond
//! what the service does, call the service, return DTOs. Compiled only when the
//! `tauri` feature is on, so the module stays testable without a webview.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::storage::Database;
use tauri::State;

use crate::domain::{Campaign, NewCampaign, UpdateCampaign};
use crate::service::CampaignService;

type SharedCampaignService = Arc<CampaignService<Database>>;

/// Command: list all campaigns.
#[tauri::command]
pub fn list_campaigns(
    service: State<'_, SharedCampaignService>,
) -> Result<Vec<Campaign>, ErrorDto> {
    service.list().map_err(ErrorDto::from)
}

/// Command: create a campaign.
#[tauri::command]
pub fn create_campaign(
    service: State<'_, SharedCampaignService>,
    input: NewCampaign,
) -> Result<Campaign, ErrorDto> {
    service.create(input).map_err(ErrorDto::from)
}

/// Command: fetch one campaign by id.
#[tauri::command]
pub fn get_campaign(
    service: State<'_, SharedCampaignService>,
    campaign_id: String,
) -> Result<Option<Campaign>, ErrorDto> {
    service.get(&campaign_id).map_err(ErrorDto::from)
}

/// Command: update a campaign's editable fields.
#[tauri::command]
pub fn update_campaign(
    service: State<'_, SharedCampaignService>,
    campaign_id: String,
    input: UpdateCampaign,
) -> Result<Option<Campaign>, ErrorDto> {
    service.update(&campaign_id, input).map_err(ErrorDto::from)
}

/// Command: delete a campaign (cascades to all its data).
#[tauri::command]
pub fn delete_campaign(
    service: State<'_, SharedCampaignService>,
    campaign_id: String,
) -> Result<bool, ErrorDto> {
    service.delete(&campaign_id).map_err(ErrorDto::from)
}
