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
    storage: Arc<S>,
}

impl<S: Storage> WorldStateService<S> {
    /// Create a service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> WorldStateService<S> {
        WorldStateService { storage }
    }

    /// The current world document for a campaign (`{}` when empty).
    pub fn get_document(&self, campaign_id: &str) -> Result<Value> {
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
        let mut changes = Vec::new();
        for mutation in mutations {
            let change = match mutation {
                StateMutation::SetWorldKey { key, value } => {
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
                        id: roleplayer_core::new_id(),
                        campaign_id: campaign_id.to_string(),
                        tool: tool.to_string(),
                        args: args.clone(),
                        before_value: before,
                        after_value: after,
                        message_id: message_id.map(|value| value.to_string()),
                        created_at: roleplayer_core::now_rfc3339(),
                    }
                }
            };
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
        repo::set_key(
            self.storage.as_ref(),
            campaign_id,
            key,
            &value,
            "manual",
            &serde_json::json!({ "key": key }),
            None,
        )
    }

    /// Manually remove a world key (direct UI edit).
    pub fn remove_key_manual(
        &self,
        campaign_id: &str,
        key: &str,
    ) -> Result<(Value, Value)> {
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
        repo::list_changes(self.storage.as_ref(), campaign_id, limit)
    }
}
