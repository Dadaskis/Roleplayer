//! The provider registry — holds every registered adapter behind `Arc`.
//!
//! The registry is the single place the app asks "which provider do I call?".
//! Adapters are registered by the [`crate::service::ProviderService`] whenever a
//! config or key changes; the turn flow reads the default here.
//!
//! Concurrency: three independent `Mutex`es guard the map, the default id, and
//! the default model. Locks are short-lived and never nested (the deliberate
//! release in `set_default`), so lock ordering cannot deadlock other threads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use roleplayer_core::errors::{AppError, Result};
use roleplayer_core::llm::LLMProvider;

/// Thread-safe collection of provider adapters + the default selection.
pub struct ProviderRegistry {
    /// Provider id -> adapter.
    ///
    /// The trait object is behind `Arc` so callers clone out a cheap handle
    /// and share one adapter across many concurrent turns.
    inner: Mutex<HashMap<String, Arc<dyn LLMProvider>>>,
    /// Which provider id is the default (used when none is explicitly chosen).
    ///
    /// A separate mutex so reads/writes of the selection never contend with
    /// adapter registration traffic.
    default_id: Mutex<Option<String>>,
    /// The model string to request against the default provider.
    ///
    /// Also separate: model selection changes independently of the id.
    default_model: Mutex<Option<String>>,
}

impl ProviderRegistry {
    /// Create an empty registry.
    pub fn new() -> ProviderRegistry {
        // All three slots start empty; the first `register()` seeds the
        // default id ("first registered provider wins").
        ProviderRegistry {
            inner: Mutex::new(HashMap::new()),
            default_id: Mutex::new(None),
            default_model: Mutex::new(None),
        }
    }

    /// Register (or replace) an adapter under its id.
    ///
    /// Re-registering the same id replaces the old adapter; registering the
    /// very first provider also makes it the default (see below).
    pub fn register(&self, provider: Arc<dyn LLMProvider>) {
        // Snapshot the id before the provider is moved into the map.
        let id = provider.id().to_string();
        // Swallow a poisoned lock: registration is best-effort by design.
        // A panicked holder is the only way this fails, and skipping a
        // register is harmless — nothing user-visible depends on it.
        if let Ok(mut inner) = self.inner.lock() {
            // Insert (or replace) under the id; the Arc keeps the old adapter
            // alive only while other callers still hold a clone of it.
            inner.insert(id.clone(), provider);
        }
        // If nothing was default yet, the first registered provider wins.
        // Unlike the insert, a poisoned default lock is treated as a bug.
        // Registration order defines the default, so a poisoned lock here
        // means configuration state is already corrupt — fail loudly.
        let mut default =
            self.default_id.lock().expect("registry default lock");
        if default.is_none() {
            // The first provider claims the default slot permanently, until
            // an explicit `set_default` overrides it.
            *default = Some(id);
        }
    }

    /// Record the model to request against the default provider.
    ///
    /// The adapter does not own its model string (it travels per-request), so
    /// the registry remembers it for consumers like the turn flow.
    ///
    /// Param: the model id string to record; it is not validated here — the
    /// provider is the authority on valid models when the request is built.
    pub fn set_default_model(&self, model: String) {
        // Swallow poisoning: the model is advisory and can be set again later.
        // Unlike default_id there is no invariant to protect, so a poisoned
        // slot degrades to "no model remembered" and can be re-set any time.
        if let Ok(mut slot) = self.default_model.lock() {
            *slot = Some(model);
        }
    }

    /// The model string for the default provider, if configured.
    ///
    /// Returns None when unset or when the lock is poisoned; callers treat
    /// both as "no model configured" and fall back to a sensible default.
    pub fn default_model(&self) -> Option<String> {
        self.default_model.lock().ok().and_then(|slot| slot.clone())
    }

    /// Fetch an adapter by id; clones the `Arc`.
    ///
    /// A poisoned inner lock yields None (treated as "not found"). The `?`
    /// early-returns None on lock failure, then `cloned()` hands the caller
    /// its own handle without keeping the map locked.
    pub fn get(&self, provider_id: &str) -> Option<Arc<dyn LLMProvider>> {
        self.inner.lock().ok()?.get(provider_id).cloned()
    }

    /// The current default adapter.
    ///
    /// Two-step lookup: read the default id, then fetch the adapter. The id
    /// lock is released before the map lock is taken, so the two mutexes are
    /// never held together here (matching `set_default`'s discipline).
    pub fn get_default(&self) -> Option<Arc<dyn LLMProvider>> {
        // Separate short-lived locks: read the id, then look up the adapter.
        // `?` unwraps both the lock (poisoned → None) and the Option itself
        // (no default set → None).
        let id = self.default_id.lock().ok()?.clone()?;
        self.get(&id)
    }

    /// All registered provider ids.
    ///
    /// Order is HashMap iteration order (not insertion order); consumers
    /// should sort if they need a stable UI ordering. Returns empty on a
    /// poisoned lock rather than panicking.
    pub fn ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|inner| inner.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Set the default provider id.
    ///
    /// Validates the id against the live registry first; unknown ids are a
    /// config error rather than being silently accepted.
    pub fn set_default(&self, provider_id: &str) -> Result<()> {
        // Validate against the live map before committing the selection.
        // Read the map to confirm the id exists before accepting it; this
        // makes the selection atomic from the caller's point of view.
        let known = self.inner.lock().expect("registry lock");
        if !known.contains_key(provider_id) {
            // Reject unknown ids as a Config error the UI can render.
            return Err(AppError::Config(format!(
                "unknown provider: {provider_id}"
            )));
        }
        // Release the inner lock before taking default_id: never hold both
        // mutexes at once, so lock ordering cannot deadlock other threads.
        // Holding both would risk a deadlock if another thread ever locked
        // them in the opposite order; dropping here keeps the ordering total.
        drop(known);
        // Validation passed, so commit the new default selection.
        let mut default = self.default_id.lock().expect("default lock");
        *default = Some(provider_id.to_string());
        Ok(())
    }

    /// Require a provider by id, returning a typed error if absent.
    ///
    /// Unlike `get`, absence is converted into a Config error so callers can
    /// `?` it straight into a command handler.
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
        // An empty registry is a valid default state; nothing to seed.
        ProviderRegistry::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;

    #[test]
    fn first_registered_provider_becomes_default() {
        // With no explicit default set, the first registered adapter must be
        // selected automatically — that is the bootstrap rule.
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
        // Setting an id that was never registered is a config error, and a
        // known id is accepted — exercising both branches of set_default.
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
