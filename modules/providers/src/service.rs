//! Provider orchestration: builds adapters from config, manages keys/defaults.

use std::sync::Arc;

use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::llm::{
    ChatMessage, CompletionRequest, LLMProvider, ModelInfo, Role,
};
use roleplayer_core::now_rfc3339;
use roleplayer_core::storage::Storage;

use crate::domain::{
    opencode_go_known_models, ProviderConfig, ProviderInfo, ProviderKind,
    OPENCODE_API_KEY_ENV, OPENCODE_GO_BASE_URL, OPENCODE_GO_DEFAULT_MODEL,
};
use crate::mock::MockProvider;
use crate::openai::OpenAiCompatibleProvider;
use crate::registry::ProviderRegistry;
use crate::secrets::Secrets;
use crate::storage as repo;

/// Id of the built-in mock provider.
pub const MOCK_PROVIDER_ID: &str = "mock";

/// Id of the built-in OpenCode Go provider.
pub const OPENCODE_GO_PROVIDER_ID: &str = "opencode-go";

/// Input for updating a provider's non-secret config.
#[derive(Debug, Clone)]
pub struct ProviderConfigInput {
    pub model: String,
    pub base_url: String,
}

/// Orchestrates providers: seeds defaults, rebuilds adapters on change.
pub struct ProviderService<S: Storage> {
    storage: Arc<S>,
    registry: Arc<ProviderRegistry>,
}

impl<S: Storage> ProviderService<S> {
    /// Create the service over the storage seam and the shared registry.
    pub fn new(
        storage: Arc<S>,
        registry: Arc<ProviderRegistry>,
    ) -> ProviderService<S> {
        ProviderService { storage, registry }
    }

    /// Seed built-in configs (mock + opencode-go) if they do not exist yet.
    pub fn ensure_defaults(&self) -> Result<()> {
        if repo::get(self.storage.as_ref(), MOCK_PROVIDER_ID)?.is_none() {
            self.persist(ProviderConfig {
                id: MOCK_PROVIDER_ID.to_string(),
                name: "Mock".to_string(),
                kind: ProviderKind::Mock,
                base_url: String::new(),
                model: "mock/model".to_string(),
                is_default: false,
                created_at: now_rfc3339(),
            })?;
        }
        if repo::get(self.storage.as_ref(), OPENCODE_GO_PROVIDER_ID)?.is_none()
        {
            self.persist(ProviderConfig {
                id: OPENCODE_GO_PROVIDER_ID.to_string(),
                name: "OpenCode Go".to_string(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: OPENCODE_GO_BASE_URL.to_string(),
                model: OPENCODE_GO_DEFAULT_MODEL.to_string(),
                is_default: false,
                created_at: now_rfc3339(),
            })?;
        }
        self.rebuild_all()?;
        // If a real key is available, prefer the real provider; else Mock.
        let opencode_config = self.require_config(OPENCODE_GO_PROVIDER_ID)?;
        if self.has_key(&opencode_config) {
            self.set_default(OPENCODE_GO_PROVIDER_ID)?;
        } else {
            self.set_default(MOCK_PROVIDER_ID)?;
        }
        tracing::info!("provider defaults ensured");
        Ok(())
    }

    /// The registry (shared with the turn flow).
    pub fn registry(&self) -> Arc<ProviderRegistry> {
        self.registry.clone()
    }

    /// All providers with UI-facing info.
    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        let configs = repo::list(self.storage.as_ref())?;
        Ok(configs.iter().map(|config| self.to_info(config)).collect())
    }

    /// Models a provider can run (live when possible, static fallback otherwise).
    pub async fn list_models(
        &self,
        provider_id: &str,
    ) -> Result<Vec<ModelInfo>> {
        let provider = self.registry.require(provider_id)?;
        provider.list_models().await
    }

    /// Update a provider's non-secret config and rebuild its adapter.
    pub fn update_config(
        &self,
        provider_id: &str,
        input: ProviderConfigInput,
    ) -> Result<ProviderInfo> {
        if input.model.trim().is_empty() {
            return Err(AppError::Config(
                "model must not be empty".to_string(),
            ));
        }
        let mut config = self.require_config(provider_id)?;
        config.model = input.model.trim().to_string();
        if !input.base_url.trim().is_empty() {
            config.base_url = input.base_url.trim().to_string();
        }
        self.persist(config.clone())?;
        self.rebuild(&config)?;
        tracing::info!(provider_id, "provider config updated");
        Ok(self.to_info(&config))
    }

    /// Store a provider's API key in the keyring and rebuild its adapter.
    pub fn set_api_key(
        &self,
        provider_id: &str,
        api_key: &str,
    ) -> Result<ProviderInfo> {
        Secrets::set(provider_id, api_key)?;
        let config = self.require_config(provider_id)?;
        self.rebuild(&config)?;
        tracing::info!(provider_id, "api key updated");
        Ok(self.to_info(&config))
    }

    /// Remove a provider's stored API key.
    pub fn clear_api_key(&self, provider_id: &str) -> Result<ProviderInfo> {
        Secrets::delete(provider_id)?;
        let config = self.require_config(provider_id)?;
        self.rebuild(&config)?;
        tracing::info!(provider_id, "api key cleared");
        Ok(self.to_info(&config))
    }

    /// Make `provider_id` the default provider.
    pub fn set_default(&self, provider_id: &str) -> Result<ProviderInfo> {
        let mut config = self.require_config(provider_id)?;
        repo::clear_defaults(self.storage.as_ref())?;
        config.is_default = true;
        self.persist(config.clone())?;
        self.registry.set_default(provider_id)?;
        self.registry.set_default_model(config.model.clone());
        self.rebuild_all()?;
        tracing::info!(provider_id, "default provider set");
        Ok(self.to_info(&config))
    }

    /// Run a tiny completion to prove a provider is configured and reachable.
    pub async fn test(&self, provider_id: &str) -> Result<String> {
        let config = self.require_config(provider_id)?;
        let provider = self.registry.require(provider_id)?;
        let request = CompletionRequest {
            model: config.model.clone(),
            messages: vec![ChatMessage::text(
                Role::User,
                "Reply with exactly: OK",
            )],
            tools: vec![],
            temperature: Some(0.0),
            max_tokens: Some(16),
            stream: false,
        };
        let response = provider.complete(request).await?;
        let text = response
            .message
            .content
            .iter()
            .filter_map(|block| block.text())
            .collect::<Vec<_>>()
            .join(" ");
        tracing::info!(provider_id, "provider test succeeded");
        Ok(text)
    }

    fn to_info(&self, config: &ProviderConfig) -> ProviderInfo {
        ProviderInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            kind: config.kind,
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            has_key: self.has_key(config),
            is_default: self
                .registry
                .get_default()
                .map(|provider| provider.id() == config.id)
                .unwrap_or(false),
        }
    }

    fn require_config(&self, provider_id: &str) -> Result<ProviderConfig> {
        repo::get(self.storage.as_ref(), provider_id)?.ok_or_else(|| {
            AppError::Config(format!("provider not configured: {provider_id}"))
        })
    }

    /// Whether a usable key exists (env override first, then keyring).
    fn has_key(&self, config: &ProviderConfig) -> bool {
        self.resolve_key(config).is_some()
    }

    fn resolve_key(&self, config: &ProviderConfig) -> Option<String> {
        // Dev convenience: the env var overrides the keyring for opencode-go.
        if config.id == OPENCODE_GO_PROVIDER_ID {
            if let Ok(value) = std::env::var(OPENCODE_API_KEY_ENV) {
                if !value.trim().is_empty() {
                    return Some(value);
                }
            }
        }
        Secrets::get(&config.id).ok().flatten()
    }

    fn persist(&self, config: ProviderConfig) -> Result<()> {
        repo::upsert(self.storage.as_ref(), &config)
    }

    fn rebuild_all(&self) -> Result<()> {
        let configs = repo::list(self.storage.as_ref())?;
        for config in &configs {
            self.rebuild(config)?;
        }
        Ok(())
    }

    /// Build an adapter for a config and register it in the registry.
    fn rebuild(&self, config: &ProviderConfig) -> Result<()> {
        let provider: Arc<dyn LLMProvider> = match config.kind {
            ProviderKind::Mock => Arc::new(MockProvider::new(
                &config.id,
                &config.model,
                "The GM nods thoughtfully.",
            )),
            ProviderKind::OpenAiCompatible => {
                let key = self.resolve_key(config).unwrap_or_default();
                Arc::new(OpenAiCompatibleProvider::new(
                    &config.id,
                    &config.base_url,
                    &key,
                    if config.id == OPENCODE_GO_PROVIDER_ID {
                        opencode_go_known_models()
                    } else {
                        vec![]
                    },
                ))
            }
        };
        self.registry.register(provider);
        if config.is_default {
            self.registry.set_default(&config.id)?;
        }
        Ok(())
    }
}
