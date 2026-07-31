//! Character orchestration: the service layer for the `characters` module.

use std::sync::Arc;

use roleplayer_core::errors::Result;
use roleplayer_core::new_id;
use roleplayer_core::now_rfc3339;
use roleplayer_core::storage::Storage;

use crate::domain::{Character, NewCharacter, UpdateCharacter};
use crate::storage as repo;

/// Orchestrates character lifecycle within a campaign.
pub struct CharacterService<S: Storage> {
    // Shared seam to the single-writer DB connection; `Arc` allows cheap
    // cloning of the service for concurrent turn-flow tasks.
    storage: Arc<S>,
}

impl<S: Storage> CharacterService<S> {
    /// Create a service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> CharacterService<S> {
        // Wrap the caller-provided backend; which backend is injected is the
        // composition root's decision (file DB vs. in-memory test DB).
        CharacterService { storage }
    }

    /// Create a character after validating input.
    pub fn create(&self, input: NewCharacter) -> Result<Character> {
        // Fail fast on invalid input before any row is written.
        input.validate()?;
        let character = Character {
            // Server-generated random UUID; clients never pick ids (§5.4).
            id: new_id(),
            // Normalize whitespace so queries and the UI see a clean string.
            campaign_id: input.campaign_id.trim().to_string(),
            name: input.name.trim().to_string(),
            // Whether this is the human player or a GM/player-controlled NPC;
            // drives roster labeling in the system prompt.
            is_player: input.is_player,
            bio: input.bio.trim().to_string(),
            stats: input.stats,
            // `extra` starts empty; future features grow it as a JSON blob
            // rather than forcing a schema change (§5.4).
            extra: serde_json::Value::Object(Default::default()),
            created_at: now_rfc3339(),
        };
        // Persist; a duplicate id would surface here as a typed error.
        repo::insert(self.storage.as_ref(), &character)?;
        // Log both ids so a character can be tied back to its campaign.
        tracing::info!(
            campaign_id = %character.campaign_id,
            character_id = %character.id,
            "character created"
        );
        Ok(character)
    }

    /// All characters of a campaign.
    pub fn list_for_campaign(
        &self,
        campaign_id: &str,
    ) -> Result<Vec<Character>> {
        // Delegated to the repo; the WHERE clause scoping to the campaign
        // lives in the SQL, keeping this service logic-free.
        repo::list_for_campaign(self.storage.as_ref(), campaign_id)
    }

    /// One character by id.
    pub fn get(&self, character_id: &str) -> Result<Option<Character>> {
        // None = not found, a normal case the caller renders as such.
        repo::get(self.storage.as_ref(), character_id)
    }

    /// Update a character's editable fields; `None` when the id is unknown.
    pub fn update(
        &self,
        character_id: &str,
        input: UpdateCharacter,
    ) -> Result<Option<Character>> {
        // Validate incoming fields before touching the DB.
        input.validate()?;
        // Load the stored row to carry over immutable fields; a missing
        // character short-circuits to None rather than erroring.
        let existing = match repo::get(self.storage.as_ref(), character_id)? {
            Some(character) => character,
            None => return Ok(None),
        };
        let updated = Character {
            // Identity and lineage fields are preserved from the stored row.
            id: existing.id,
            campaign_id: existing.campaign_id,
            name: input.name.trim().to_string(),
            is_player: input.is_player,
            bio: input.bio.trim().to_string(),
            // Only name/bio/is_player/stats are editable; extra carries over.
            stats: input.stats,
            extra: existing.extra,
            created_at: existing.created_at,
        };
        repo::update(self.storage.as_ref(), &updated)?;
        tracing::info!(character_id = %character_id, "character updated");
        Ok(Some(updated))
    }

    /// Delete a character; `true` if it existed.
    pub fn delete(&self, character_id: &str) -> Result<bool> {
        // Reports existence so the UI can confirm the deletion happened.
        let deleted = repo::delete(self.storage.as_ref(), character_id)?;
        tracing::info!(character_id = %character_id, deleted, "character deleted");
        Ok(deleted)
    }
}
