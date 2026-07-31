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
    // The model identifier the adapter will send on every completion request.
    pub model: String,
    // The endpoint base URL for OpenAI-compatible providers.
    pub base_url: String,
}

/// Orchestrates providers: seeds defaults, rebuilds adapters on change.
pub struct ProviderService<S: Storage> {
    // Persisted provider configs live in the DB (never the API keys, which
    // go to the OS keyring per §5.4).
    storage: Arc<S>,
    // Live adapter cache shared with the turn flow; rebuilt on any change so
    // it always matches the persisted config.
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
        // The mock is the reference implementation and the offline fallback;
        // opencode-go is the default real provider. Both are seeded so the app
        // is usable before any user configuration exists.
        if repo::get(self.storage.as_ref(), MOCK_PROVIDER_ID)?.is_none() {
            // Mock needs no key and no endpoint; it exists so the app works
            // with zero configuration and offline.
            self.persist(ProviderConfig {
                id: MOCK_PROVIDER_ID.to_string(),
                name: "Mock".to_string(),
                kind: ProviderKind::Mock,
                base_url: String::new(),
                model: "mock/model".to_string(),
                // Not the default yet; the default is chosen below based on
                // whether a real key exists.
                is_default: false,
                created_at: now_rfc3339(),
            })?;
        }
        if repo::get(self.storage.as_ref(), OPENCODE_GO_PROVIDER_ID)?.is_none()
        {
            // Seed opencode-go with its known endpoint + model so the first
            // real completion needs only an API key, not endpoint knowledge.
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
        // Rebuild every adapter from the seeded configs so the registry is in
        // a consistent, live state before the default is chosen below.
        self.rebuild_all()?;
        // Prefer opencode-go whenever a key exists: mock output is canned and
        // obviously not a real GM, so a configured real provider is strictly
        // better. Mock stays the default only when there is no key at all —
        // the app must still work out of the box.
        // If a real key is available, prefer the real provider; else Mock.
        let opencode_config = self.require_config(OPENCODE_GO_PROVIDER_ID)?;
        if self.has_key(&opencode_config) {
            // A usable key exists (env override or keyring): real GM it is.
            self.set_default(OPENCODE_GO_PROVIDER_ID)?;
        } else {
            // No key anywhere: keep the offline reference implementation.
            self.set_default(MOCK_PROVIDER_ID)?;
        }
        tracing::info!("provider defaults ensured");
        Ok(())
    }

    /// The registry (shared with the turn flow).
    pub fn registry(&self) -> Arc<ProviderRegistry> {
        // Clone the Arc: the turn flow and the UI commands both hold a
        // handle to the same live adapter cache.
        self.registry.clone()
    }

    /// All providers with UI-facing info.
    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        let configs = repo::list(self.storage.as_ref())?;
        // Map every persisted config to its UI shape; key-presence is
        // resolved per provider inside to_info.
        Ok(configs.iter().map(|config| self.to_info(config)).collect())
    }

    /// Models a provider can run (live when possible, static fallback otherwise).
    pub async fn list_models(
        &self,
        provider_id: &str,
    ) -> Result<Vec<ModelInfo>> {
        // The registry holds the live adapter; a missing adapter errors here
        // before any network call is attempted.
        let provider = self.registry.require(provider_id)?;
        provider.list_models().await
    }

    /// Update a provider's non-secret config and rebuild its adapter.
    pub fn update_config(
        &self,
        provider_id: &str,
        input: ProviderConfigInput,
    ) -> Result<ProviderInfo> {
        // Reject a blank model up front: every completion needs a model, so a
        // config that cannot produce one is refused early.
        if input.model.trim().is_empty() {
            return Err(AppError::Config(
                "model must not be empty".to_string(),
            ));
        }
        // Load the current row so unmentioned fields survive the update.
        let mut config = self.require_config(provider_id)?;
        // Normalize whitespace on the way in for a clean stored value.
        config.model = input.model.trim().to_string();
        // Only override the endpoint when the client supplied a non-blank
        // one; a blank base_url means "keep what we have".
        if !input.base_url.trim().is_empty() {
            config.base_url = input.base_url.trim().to_string();
        }
        // Persist first, then rebuild — so a crash between the two leaves the
        // stored config as the source of truth for the next restart.
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
        // Keys live only in the OS keyring, never in the DB or config (§5.4).
        Secrets::set(provider_id, api_key)?;
        let config = self.require_config(provider_id)?;
        // The running adapter captured the old key; rebuild to pick it up.
        self.rebuild(&config)?;
        tracing::info!(provider_id, "api key updated");
        Ok(self.to_info(&config))
    }

    /// Remove a provider's stored API key.
    pub fn clear_api_key(&self, provider_id: &str) -> Result<ProviderInfo> {
        // Delete from the keyring; a missing entry deletes as a no-op.
        Secrets::delete(provider_id)?;
        let config = self.require_config(provider_id)?;
        // Rebuild so the live adapter drops the key it had captured; a
        // subsequent call will then fail with a clear "missing key" error.
        self.rebuild(&config)?;
        tracing::info!(provider_id, "api key cleared");
        Ok(self.to_info(&config))
    }

    /// Make `provider_id` the default provider.
    pub fn set_default(&self, provider_id: &str) -> Result<ProviderInfo> {
        let mut config = self.require_config(provider_id)?;
        // Only one provider can be default; clear the flag on all others first.
        repo::clear_defaults(self.storage.as_ref())?;
        config.is_default = true;
        self.persist(config.clone())?;
        // Update both registry pointers so the turn flow and the UI agree on
        // which adapter + model is "default" right now.
        self.registry.set_default(provider_id)?;
        self.registry.set_default_model(config.model.clone());
        // Rebuild all adapters so the newly-default one carries is_default
        // flags consistent with the persisted row.
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
            // Deterministic probe: zero temperature and a 16-token budget prove
            // connectivity + key validity at minimal cost.
            temperature: Some(0.0),
            max_tokens: Some(16),
            stream: false,
        };
        // One-shot completion (no streaming) keeps the probe simple.
        let response = provider.complete(request).await?;
        // A response may mix text blocks; concatenate the text-only parts for
        // a single readable probe result.
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

    /// Map a stored config to the UI-facing provider info.
    fn to_info(&self, config: &ProviderConfig) -> ProviderInfo {
        ProviderInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            kind: config.kind,
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            // Key presence is resolved live (env/keyring), not stored, so the
            // UI always reflects what the next provider call would use.
            has_key: self.has_key(config),
            // The default flag reflects the registry's live state rather than
            // the persisted row, so it can't drift after a runtime switch.
            is_default: self
                .registry
                .get_default()
                .map(|provider| provider.id() == config.id)
                .unwrap_or(false),
        }
    }

    /// Load a provider config or fail with a descriptive config error.
    fn require_config(&self, provider_id: &str) -> Result<ProviderConfig> {
        repo::get(self.storage.as_ref(), provider_id)?.ok_or_else(|| {
            // Unknown id = unconfigured provider; surfaced as a user-facing
            // config error, not a panic.
            AppError::Config(format!("provider not configured: {provider_id}"))
        })
    }

    /// Whether a usable key exists (env override first, then keyring).
    fn has_key(&self, config: &ProviderConfig) -> bool {
        // Any resolved key counts as present, even an empty-ish one that the
        // adapter will fail on at call time — presence here drives UI state.
        self.resolve_key(config).is_some()
    }

    /// Resolve the effective key for a provider, env override first.
    fn resolve_key(&self, config: &ProviderConfig) -> Option<String> {
        // Dev convenience: the env var overrides the keyring for opencode-go,
        // letting CI and local demos run without touching the OS keyring.
        if config.id == OPENCODE_GO_PROVIDER_ID {
            if let Ok(value) = std::env::var(OPENCODE_API_KEY_ENV) {
                if !value.trim().is_empty() {
                    // Non-blank env var wins over anything in the keyring.
                    return Some(value);
                }
            }
        }
        // Keyring lookup is best-effort: a missing or unreadable entry counts
        // as "no key", letting callers degrade gracefully instead of failing
        // on the OS keyring.
        Secrets::get(&config.id).ok().flatten()
    }

    /// Upsert a provider config row.
    fn persist(&self, config: ProviderConfig) -> Result<()> {
        repo::upsert(self.storage.as_ref(), &config)
    }

    /// Rebuild every registered adapter from the persisted configs.
    fn rebuild_all(&self) -> Result<()> {
        let configs = repo::list(self.storage.as_ref())?;
        // Rebuild in a fixed iteration order; each call re-registers its
        // adapter, and later ones overwrite earlier ones for the same id.
        for config in &configs {
            self.rebuild(config)?;
        }
        Ok(())
    }

    /// Build an adapter for a config and register it in the registry.
    fn rebuild(&self, config: &ProviderConfig) -> Result<()> {
        // Every config/key change goes through here, so the registry always
        // holds adapters that match the latest persisted settings.
        let provider: Arc<dyn LLMProvider> = match config.kind {
            ProviderKind::Mock => Arc::new(MockProvider::new(
                &config.id,
                &config.model,
                "The GM nods thoughtfully.",
            )),
            ProviderKind::OpenAiCompatible => {
                // A missing key degrades to an empty string: the adapter builds
                // fine and the first real call fails with a clear provider
                // error, so the settings UI stays usable before setup.
                let key = self.resolve_key(config).unwrap_or_default();
                Arc::new(OpenAiCompatibleProvider::new(
                    &config.id,
                    &config.base_url,
                    &key,
                    // Only opencode-go ships a known static model list, so its
                    // UI can show models without a live API call; other
                    // OpenAI-compatible endpoints query the server at runtime.
                    if config.id == OPENCODE_GO_PROVIDER_ID {
                        opencode_go_known_models()
                    } else {
                        vec![]
                    },
                ))
            }
        };
        // Re-registering overwrites any previous adapter for this id, keeping
        // the registry free of stale configs.
        self.registry.register(provider);
        // Keep the registry's default pointer aligned with the persisted row
        // so a freshly rebuilt default config stays authoritative.
        if config.is_default {
            self.registry.set_default(&config.id)?;
        }
        Ok(())
    }
}
