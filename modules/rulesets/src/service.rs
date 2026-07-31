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
    // Shared seam to the single-writer DB connection; `Arc` keeps the service
    // cheaply shareable across concurrent turn-flow tasks.
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
        // Read all rulesets so we can detect the built-in by name. This is a
        // small table, so a full scan at startup is cheaper than a second
        // indexed query.
        let existing = repo::list(self.storage.as_ref())?;
        // Name (not id) is the identity here: the default ships with a fixed
        // name, so a renamed user copy must not block the seed.
        if existing.iter().any(|ruleset| ruleset.name == DEFAULT_RULESET_NAME) {
            // Already seeded (or user created a same-named ruleset): bail out
            // quietly, keeping the call idempotent across restarts.
            return Ok(());
        }
        let ruleset = Ruleset {
            // Random server-generated id, like every other entity.
            id: new_id(),
            // Use the canonical constant so the name stays stable forever.
            name: DEFAULT_RULESET_NAME.to_string(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            // No world rules for the built-in; it's just the default prompt.
            world_rules: serde_json::json!({}),
            // Marked built-in so updates and deletes are refused later (§5.16:
            // no destructive ops on the app's own seed data).
            is_builtin: true,
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &ruleset)?;
        tracing::info!(ruleset_id = %ruleset.id, "seeded default ruleset");
        Ok(())
    }

    /// Create a custom ruleset after validating input.
    pub fn create(&self, input: NewRuleset) -> Result<Ruleset> {
        // Reject invalid input before any DB write.
        input.validate()?;
        let ruleset = Ruleset {
            id: new_id(),
            // Trim so names and prompts don't carry accidental whitespace.
            name: input.name.trim().to_string(),
            system_prompt: input.system_prompt.trim().to_string(),
            // World rules are a free-form JSON document, not a typed struct
            // (§5.4: any kind of data starts as JSON).
            world_rules: input.world_rules,
            // User-created rulesets are never built-in, so they stay editable.
            is_builtin: false,
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &ruleset)?;
        tracing::info!(ruleset_id = %ruleset.id, "ruleset created");
        Ok(ruleset)
    }

    /// All rulesets, built-ins first.
    pub fn list(&self) -> Result<Vec<Ruleset>> {
        // Delegated; the "built-ins first" ordering lives in the SQL.
        repo::list(self.storage.as_ref())
    }

    /// One ruleset by id.
    pub fn get(&self, ruleset_id: &str) -> Result<Option<Ruleset>> {
        // None = not found; the caller decides how to present that to the UI.
        repo::get(self.storage.as_ref(), ruleset_id)
    }

    /// Update a custom ruleset; `None` when unknown or built-in.
    pub fn update(
        &self,
        ruleset_id: &str,
        input: UpdateRuleset,
    ) -> Result<Option<Ruleset>> {
        input.validate()?;
        // Load the stored row to carry over immutable fields; a missing
        // ruleset short-circuits to None rather than erroring.
        let existing = match repo::get(self.storage.as_ref(), ruleset_id)? {
            Some(ruleset) => ruleset,
            None => return Ok(None),
        };
        // Built-ins are immutable; updates to them are refused, not applied.
        // Returning None here lets the UI show the ruleset unchanged rather
        // than surfacing a confusing error for an "allowed" edit.
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
        // Reports whether a row existed, so the UI can confirm the outcome.
        let deleted = repo::delete(self.storage.as_ref(), ruleset_id)?;
        tracing::info!(ruleset_id = %ruleset_id, deleted, "ruleset delete requested");
        Ok(deleted)
    }
}
