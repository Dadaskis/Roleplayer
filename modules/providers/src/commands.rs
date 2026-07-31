//! Thin Tauri IPC commands for the `providers` module.

use std::sync::Arc;

use roleplayer_core::errors::ErrorDto;
use roleplayer_core::llm::ModelInfo;
use roleplayer_core::storage::Database;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::domain::ProviderInfo;
use crate::service::{ProviderConfigInput, ProviderService};

// Long-lived shared instance, injected via Tauri State; Arc for concurrency.
// `Database` is the concrete backend the app wires at startup.
type SharedProviderService = Arc<ProviderService<Database>>;

/// Wire shape for updating a provider's config.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderConfigRequest {
    pub model: String,
    // base_url is optional on the wire so clients can update just the model.
    #[serde(default)]
    pub base_url: String,
}

/// Command: list all providers with UI-facing info.
#[tauri::command]
pub fn list_providers(
    service: State<'_, SharedProviderService>,
) -> Result<Vec<ProviderInfo>, ErrorDto> {
    // Delegation only; has_key/is_default are resolved inside the service.
    service.list_providers().map_err(ErrorDto::from)
}

/// Command: list models a provider can run.
#[tauri::command]
pub async fn list_models(
    service: State<'_, SharedProviderService>,
    provider_id: String,
) -> Result<Vec<ModelInfo>, ErrorDto> {
    // Async: the provider may query a live API endpoint for the model list.
    service.list_models(&provider_id).await.map_err(ErrorDto::from)
}

/// Command: update a provider's model/base_url.
#[tauri::command]
pub fn set_provider_config(
    service: State<'_, SharedProviderService>,
    provider_id: String,
    // Deserialized from JSON; base_url may be absent (serde default).
    input: ProviderConfigRequest,
) -> Result<ProviderInfo, ErrorDto> {
    service
        .update_config(
            &provider_id,
            ProviderConfigInput {
                // The wire struct is mapped onto the service input struct;
                // both are plain data, so no validation is duplicated here.
                model: input.model,
                base_url: input.base_url,
            },
        )
        .map_err(ErrorDto::from)
}

/// Command: store a provider's API key in the keyring.
#[tauri::command]
pub fn set_provider_api_key(
    service: State<'_, SharedProviderService>,
    provider_id: String,
    // The key travels over IPC once and is stored only in the OS keyring,
    // never logged or persisted to the DB (§5.4, §5.13).
    api_key: String,
) -> Result<ProviderInfo, ErrorDto> {
    service.set_api_key(&provider_id, &api_key).map_err(ErrorDto::from)
}

/// Command: clear a provider's stored API key.
#[tauri::command]
pub fn clear_provider_api_key(
    service: State<'_, SharedProviderService>,
    provider_id: String,
) -> Result<ProviderInfo, ErrorDto> {
    service.clear_api_key(&provider_id).map_err(ErrorDto::from)
}

/// Command: make a provider the default.
#[tauri::command]
pub fn set_default_provider(
    service: State<'_, SharedProviderService>,
    provider_id: String,
) -> Result<ProviderInfo, ErrorDto> {
    service.set_default(&provider_id).map_err(ErrorDto::from)
}

/// Command: run a tiny test completion against a provider.
#[tauri::command]
pub async fn test_provider(
    service: State<'_, SharedProviderService>,
    provider_id: String,
) -> Result<String, ErrorDto> {
    // Async because the probe performs a real provider round-trip; the caller
    // blocks on the result rather than on a sync command thread.
    service.test(&provider_id).await.map_err(ErrorDto::from)
}
