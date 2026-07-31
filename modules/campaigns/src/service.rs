//! Campaign orchestration: the service layer for the `campaigns` module.

use std::sync::Arc;

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use roleplayer_core::{new_id, now_rfc3339};

use crate::domain::{Campaign, NewCampaign, UpdateCampaign};
use crate::storage as repo;

/// Orchestrates campaign lifecycle: validates, persists, and returns entities.
///
/// Generic over `S: Storage` so the same service works against the file DB and
/// the in-memory test DB (§5.11 of AGENTS.md). Thin by design (§5.2): no
/// business decisions live here beyond applying the domain rules.
pub struct CampaignService<S: Storage> {
    storage: Arc<S>,
}

impl<S: Storage> CampaignService<S> {
    /// Create a service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> CampaignService<S> {
        CampaignService { storage }
    }

    /// Create a campaign after validating its input.
    pub fn create(&self, input: NewCampaign) -> Result<Campaign> {
        input.validate()?;
        let now = now_rfc3339();
        let campaign = Campaign {
            id: new_id(),
            name: input.name.trim().to_string(),
            description: input.description.trim().to_string(),
            ruleset_id: input.ruleset_id,
            settings: serde_json::Value::Object(Default::default()),
            created_at: now.clone(),
            updated_at: now,
        };
        repo::insert(self.storage.as_ref(), &campaign)?;
        tracing::info!(campaign_id = %campaign.id, "campaign created");
        Ok(campaign)
    }

    /// All campaigns, newest first.
    pub fn list(&self) -> Result<Vec<Campaign>> {
        repo::list(self.storage.as_ref())
    }

    /// One campaign by id.
    pub fn get(&self, campaign_id: &str) -> Result<Option<Campaign>> {
        repo::get(self.storage.as_ref(), campaign_id)
    }

    /// Update a campaign's editable fields; `None` when the id is unknown.
    pub fn update(
        &self,
        campaign_id: &str,
        input: UpdateCampaign,
    ) -> Result<Option<Campaign>> {
        input.validate()?;
        let existing = match repo::get(self.storage.as_ref(), campaign_id)? {
            Some(campaign) => campaign,
            None => return Ok(None),
        };
        let updated = Campaign {
            id: existing.id.clone(),
            name: input.name.trim().to_string(),
            description: input.description.trim().to_string(),
            ruleset_id: input.ruleset_id,
            settings: existing.settings,
            created_at: existing.created_at,
            updated_at: now_rfc3339(),
        };
        repo::update(self.storage.as_ref(), &updated, &updated.updated_at)?;
        tracing::info!(campaign_id = %campaign_id, "campaign updated");
        Ok(Some(updated))
    }

    /// Delete a campaign (cascades to its children). `true` if it existed.
    pub fn delete(&self, campaign_id: &str) -> Result<bool> {
        let deleted = repo::delete(self.storage.as_ref(), campaign_id)?;
        tracing::info!(campaign_id = %campaign_id, deleted, "campaign delete requested");
        Ok(deleted)
    }
}
