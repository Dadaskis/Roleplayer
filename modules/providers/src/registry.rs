//! The provider registry — holds every registered adapter behind `Arc`.
//!
//! The registry is the single place the app asks "which provider do I call?".
//! Adapters are registered by the [`crate::service::ProviderService`] whenever a
//! config or key changes; the turn flow reads the default here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::llm::LLMProvider;

/// Thread-safe collection of provider adapters + the default selection.
pub struct ProviderRegistry {
    /// Provider id -> adapter.
    inner: Mutex<HashMap<String, Arc<dyn LLMProvider>>>,
    /// Which provider id is the default (used when none is explicitly chosen).
    default_id: Mutex<Option<String>>,
    /// The model string to request against the default provider.
    default_model: Mutex<Option<String>>,
}

impl ProviderRegistry {
    /// Create an empty registry.
    pub fn new() -> ProviderRegistry {
        ProviderRegistry {
            inner: Mutex::new(HashMap::new()),
            default_id: Mutex::new(None),
            default_model: Mutex::new(None),
        }
    }

    /// Register (or replace) an adapter under its id.
    pub fn register(&self, provider: Arc<dyn LLMProvider>) {
        let id = provider.id().to_string();
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(id.clone(), provider);
        }
        // If nothing was default yet, the first registered provider wins.
        let mut default =
            self.default_id.lock().expect("registry default lock");
        if default.is_none() {
            *default = Some(id);
        }
    }

    /// Record the model to request against the default provider.
    ///
    /// The adapter does not own its model string (it travels per-request), so
    /// the registry remembers it for consumers like the turn flow.
    pub fn set_default_model(&self, model: String) {
        if let Ok(mut slot) = self.default_model.lock() {
            *slot = Some(model);
        }
    }

    /// The model string for the default provider, if configured.
    pub fn default_model(&self) -> Option<String> {
        self.default_model.lock().ok().and_then(|slot| slot.clone())
    }

    /// Fetch an adapter by id; clones the `Arc`.
    pub fn get(&self, provider_id: &str) -> Option<Arc<dyn LLMProvider>> {
        self.inner.lock().ok()?.get(provider_id).cloned()
    }

    /// The current default adapter.
    pub fn get_default(&self) -> Option<Arc<dyn LLMProvider>> {
        let id = self.default_id.lock().ok()?.clone()?;
        self.get(&id)
    }

    /// All registered provider ids.
    pub fn ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|inner| inner.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Set the default provider id.
    pub fn set_default(&self, provider_id: &str) -> Result<()> {
        let known = self.inner.lock().expect("registry lock");
        if !known.contains_key(provider_id) {
            return Err(AppError::Config(format!(
                "unknown provider: {provider_id}"
            )));
        }
        drop(known);
        let mut default = self.default_id.lock().expect("default lock");
        *default = Some(provider_id.to_string());
        Ok(())
    }

    /// Require a provider by id, returning a typed error if absent.
    pub fn require(&self, provider_id: &str) -> Result<Arc<dyn LLMProvider>> {
        self.get(provider_id).ok_or_else(|| {
            AppError::Config(format!("provider not configured: {provider_id}"))
        })
    }

    /// Require the default provider, returning a typed error if absent.
    pub fn require_default(&self) -> Result<Arc<dyn LLMProvider>> {
        self.get_default().ok_or_else(|| {
            AppError::Config("no default provider configured".to_string())
        })
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        ProviderRegistry::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;

    #[test]
    fn first_registered_provider_becomes_default() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new(
            "mock",
            "mock/model",
            "hi",
        )));
        let default = registry.get_default().expect("has default");
        assert_eq!(default.id(), "mock");
    }

    #[test]
    fn set_default_rejects_unknown_ids() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new(
            "mock",
            "mock/model",
            "hi",
        )));
        assert!(registry.set_default("nope").is_err());
        assert!(registry.set_default("mock").is_ok());
    }
}
