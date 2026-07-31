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

// Long-lived shared instance, injected via Tauri State; Arc for concurrency.
// `Database` is the concrete backend the app wires at startup.
type SharedCampaignService = Arc<CampaignService<Database>>;

/// Command: list all campaigns.
#[tauri::command]
pub fn list_campaigns(
    // Tauri injects the shared service from managed state; the borrow is for
    // this one command call only.
    service: State<'_, SharedCampaignService>,
) -> Result<Vec<Campaign>, ErrorDto> {
    // Thin delegation: the service owns ordering; the error mapping to the
    // wire DTO happens at this boundary so the UI never sees a raw error.
    service.list().map_err(ErrorDto::from)
}

/// Command: create a campaign.
#[tauri::command]
pub fn create_campaign(
    service: State<'_, SharedCampaignService>,
    // The deserialized input from the webview is validated inside the
    // service (NewCampaign::validate), not here.
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
    // None (unknown id) travels over the wire as JSON null, not an error.
    service.get(&campaign_id).map_err(ErrorDto::from)
}

/// Command: update a campaign's editable fields.
#[tauri::command]
pub fn update_campaign(
    service: State<'_, SharedCampaignService>,
    // The id is a lookup key only; ownership data always comes from the DB.
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
    // The boolean answers "did it exist?", letting the UI tell a successful
    // cascade apart from a no-op on an already-deleted campaign.
    service.delete(&campaign_id).map_err(ErrorDto::from)
}
