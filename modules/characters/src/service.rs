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
    storage: Arc<S>,
}

impl<S: Storage> CharacterService<S> {
    /// Create a service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> CharacterService<S> {
        CharacterService { storage }
    }

    /// Create a character after validating input.
    pub fn create(&self, input: NewCharacter) -> Result<Character> {
        input.validate()?;
        let character = Character {
            id: new_id(),
            campaign_id: input.campaign_id.trim().to_string(),
            name: input.name.trim().to_string(),
            is_player: input.is_player,
            bio: input.bio.trim().to_string(),
            stats: input.stats,
            extra: serde_json::Value::Object(Default::default()),
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &character)?;
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
        repo::list_for_campaign(self.storage.as_ref(), campaign_id)
    }

    /// One character by id.
    pub fn get(&self, character_id: &str) -> Result<Option<Character>> {
        repo::get(self.storage.as_ref(), character_id)
    }

    /// Update a character's editable fields; `None` when the id is unknown.
    pub fn update(
        &self,
        character_id: &str,
        input: UpdateCharacter,
    ) -> Result<Option<Character>> {
        input.validate()?;
        let existing = match repo::get(self.storage.as_ref(), character_id)? {
            Some(character) => character,
            None => return Ok(None),
        };
        let updated = Character {
            id: existing.id,
            campaign_id: existing.campaign_id,
            name: input.name.trim().to_string(),
            is_player: input.is_player,
            bio: input.bio.trim().to_string(),
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
        let deleted = repo::delete(self.storage.as_ref(), character_id)?;
        tracing::info!(character_id = %character_id, deleted, "character deleted");
        Ok(deleted)
    }
}
