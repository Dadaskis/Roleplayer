//! Ruleset orchestration: the service layer for the `rulesets` module.

use std::sync::Arc;

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use roleplayer_core::{new_id, now_rfc3339};

use crate::domain::{
    NewRuleset, Ruleset, UpdateRuleset, DEFAULT_RULESET_NAME,
    DEFAULT_SYSTEM_PROMPT,
};
use crate::storage as repo;

/// Orchestrates ruleset lifecycle and seeds the built-in default.
pub struct RulesetService<S: Storage> {
    storage: Arc<S>,
}

impl<S: Storage> RulesetService<S> {
    /// Create a service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> RulesetService<S> {
        RulesetService { storage }
    }

    /// Seed the built-in default ruleset if none exists yet.
    ///
    /// Called once at startup (composition root). Idempotent: if any ruleset
    /// named [`DEFAULT_RULESET_NAME`] already exists, nothing happens.
    pub fn ensure_default(&self) -> Result<()> {
        let existing = repo::list(self.storage.as_ref())?;
        if existing.iter().any(|ruleset| ruleset.name == DEFAULT_RULESET_NAME) {
            return Ok(());
        }
        let ruleset = Ruleset {
            id: new_id(),
            name: DEFAULT_RULESET_NAME.to_string(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            world_rules: serde_json::json!({}),
            is_builtin: true,
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &ruleset)?;
        tracing::info!(ruleset_id = %ruleset.id, "seeded default ruleset");
        Ok(())
    }

    /// Create a custom ruleset after validating input.
    pub fn create(&self, input: NewRuleset) -> Result<Ruleset> {
        input.validate()?;
        let ruleset = Ruleset {
            id: new_id(),
            name: input.name.trim().to_string(),
            system_prompt: input.system_prompt.trim().to_string(),
            world_rules: input.world_rules,
            is_builtin: false,
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &ruleset)?;
        tracing::info!(ruleset_id = %ruleset.id, "ruleset created");
        Ok(ruleset)
    }

    /// All rulesets, built-ins first.
    pub fn list(&self) -> Result<Vec<Ruleset>> {
        repo::list(self.storage.as_ref())
    }

    /// One ruleset by id.
    pub fn get(&self, ruleset_id: &str) -> Result<Option<Ruleset>> {
        repo::get(self.storage.as_ref(), ruleset_id)
    }

    /// Update a custom ruleset; `None` when unknown or built-in.
    pub fn update(
        &self,
        ruleset_id: &str,
        input: UpdateRuleset,
    ) -> Result<Option<Ruleset>> {
        input.validate()?;
        let existing = match repo::get(self.storage.as_ref(), ruleset_id)? {
            Some(ruleset) => ruleset,
            None => return Ok(None),
        };
        if existing.is_builtin {
            return Ok(None);
        }
        let updated = Ruleset {
            id: existing.id,
            name: input.name.trim().to_string(),
            system_prompt: input.system_prompt.trim().to_string(),
            world_rules: input.world_rules,
            is_builtin: false,
            created_at: existing.created_at,
        };
        repo::update(self.storage.as_ref(), &updated)?;
        tracing::info!(ruleset_id = %ruleset_id, "ruleset updated");
        Ok(Some(updated))
    }

    /// Delete a custom ruleset; `true` if one was deleted.
    pub fn delete(&self, ruleset_id: &str) -> Result<bool> {
        let deleted = repo::delete(self.storage.as_ref(), ruleset_id)?;
        tracing::info!(ruleset_id = %ruleset_id, deleted, "ruleset delete requested");
        Ok(deleted)
    }
}
