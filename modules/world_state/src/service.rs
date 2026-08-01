//! World-state orchestration: applies mutations and exposes the audit trail.

use std::sync::Arc;

use roleplayer_core::errors::Result;
use roleplayer_core::game_command::StateMutation;
use roleplayer_core::storage::Storage;
use serde_json::Value;

use crate::domain::StateChange;
use crate::storage as repo;

/// Orchestrates the world-state document. The only applier of mutations.
pub struct WorldStateService<S: Storage> {
    // Shared seam to the single-writer DB connection; `Arc` allows the service
    // to be shared between the turn flow and the UI command handlers.
    storage: Arc<S>,
}

impl<S: Storage> WorldStateService<S> {
    /// Create a service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> WorldStateService<S> {
        WorldStateService { storage }
    }

    /// The current world document for a campaign (`{}` when empty).
    pub fn get_document(&self, campaign_id: &str) -> Result<Value> {
        // Reads the whole JSON document in one shot; per-key reads happen
        // inside the repo when the turn flow builds its context snapshot.
        repo::get_document(self.storage.as_ref(), campaign_id)
    }

    /// Apply a batch of mutations from a game command, recording each change.
    ///
    /// Used by the turn flow after a tool call. Returns the audit entries that
    /// were written so the caller can show what changed.
    pub fn apply_mutations(
        &self,
        campaign_id: &str,
        mutations: &[StateMutation],
        tool: &str,
        args: &Value,
        message_id: Option<&str>,
    ) -> Result<Vec<StateChange>> {
        // Mutations apply in declaration order; each gets its own audit row so
        // the trail shows exactly what changed, before and after.
        let mut changes = Vec::new();
        for mutation in mutations {
            // Match on the mutation kind to drive the correct repo write.
            let change = match mutation {
                StateMutation::SetWorldKey { key, value } => {
                    // Each set produces a (before, after) pair from the repo,
                    // captured right around the write for an accurate diff.
                    let (before, after) = repo::set_key(
                        self.storage.as_ref(),
                        campaign_id,
                        key,
                        value,
                        tool,
                        args,
                        message_id,
                    )?;
                    StateChange {
                        // Fresh audit id per row, never reused.
                        id: roleplayer_core::new_id(),
                        campaign_id: campaign_id.to_string(),
                        tool: tool.to_string(),
                        args: args.clone(),
                        before_value: before,
                        after_value: after,
                        // Tie the audit row to the transcript row when the
                        // change came from a tool call; None for manual edits.
                        message_id: message_id.map(|value| value.to_string()),
                        created_at: roleplayer_core::now_rfc3339(),
                    }
                }
                // Any other mutation kind reaching this applier is a routing
                // bug in the caller (turnflow routes character creations to
                // the characters service, never here) — surface it loudly
                // instead of silently skipping the write.
                StateMutation::CreateCharacter { .. } => {
                    return Err(roleplayer_core::errors::AppError::Domain(
                        "world applier cannot handle character mutations"
                            .to_string(),
                    ));
                }
            };
            // Collect every audit entry so the caller gets the full batch.
            changes.push(change);
        }
        tracing::info!(
            campaign_id = %campaign_id,
            tool,
            changes = changes.len(),
            "applied world mutations"
        );
        Ok(changes)
    }

    /// Manually set a world key (direct UI edit, not a tool call).
    pub fn set_key_manual(
        &self,
        campaign_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(Value, Value)> {
        // Audit as tool "manual" with the key as args, so manual edits produce
        // the same before/after rows a GM tool call would.
        repo::set_key(
            self.storage.as_ref(),
            campaign_id,
            key,
            &value,
            "manual",
            &serde_json::json!({ "key": key }),
            // No transcript row to link: this edit came from the settings UI,
            // not from an assistant tool call.
            None,
        )
    }

    /// Manually remove a world key (direct UI edit).
    pub fn remove_key_manual(
        &self,
        campaign_id: &str,
        key: &str,
    ) -> Result<(Value, Value)> {
        // Same uniform audit trail as set_key_manual, for removals.
        repo::remove_key(
            self.storage.as_ref(),
            campaign_id,
            key,
            "manual",
            &serde_json::json!({ "key": key }),
            None,
        )
    }

    /// Recent audit entries for a campaign, newest first.
    pub fn list_changes(
        &self,
        campaign_id: &str,
        limit: i64,
    ) -> Result<Vec<StateChange>> {
        // Delegated; the repo enforces the limit in SQL, so a huge limit
        // cannot dump unbounded rows into memory.
        repo::list_changes(self.storage.as_ref(), campaign_id, limit)
    }
}
