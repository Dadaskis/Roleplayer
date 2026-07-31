//! SQLite repository for characters. Parameterized queries only (§5.16).

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::Character;

fn map_character(row: &Row<'_>) -> rusqlite::Result<Character> {
    // Translate one SQLite row into a domain entity; columns are fetched by
    // name so the mapping survives column reordering.
    Ok(Character {
        id: row.get("id")?,
        campaign_id: row.get("campaign_id")?,
        name: row.get("name")?,
        // SQLite has no boolean type; is_player is stored as 0/1 integer.
        is_player: row.get::<_, i64>("is_player")? != 0,
        bio: row.get("bio")?,
        // Stats/extra are JSON text; parse back with a `{}` fallback so a
        // corrupt row renders as an empty doc instead of crashing (§5.10).
        stats: row
            .get::<_, String>("stats")?
            .parse()
            .unwrap_or(serde_json::Value::Object(Default::default())),
        extra: row
            .get::<_, String>("extra")?
            .parse()
            .unwrap_or(serde_json::Value::Object(Default::default())),
        created_at: row.get("created_at")?,
    })
}

/// Insert a character row.
pub fn insert<S: Storage + ?Sized>(
    storage: &S,
    character: &Character,
) -> Result<()> {
    // JSON docs are stored as text; serialize both before binding.
    let stats = character.stats.to_string();
    let extra = character.extra.to_string();
    // Placeholders ?1..?8 bind in column order; binding, never string-building,
    // keeps user data out of the SQL text (§5.16).
    storage
        .execute(
            "INSERT INTO characters (id, campaign_id, name, is_player, bio, stats, extra, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                &character.id,
                &character.campaign_id,
                // The bool is widened to an integer column; 1 = player, 0 = NPC.
                &character.name,
                &(character.is_player as i64),
                &character.bio,
                &stats,
                &extra,
                &character.created_at,
            ],
        )
        // Row count is uninteresting for an insert; map to unit.
        .map(|_changed| ())
}

/// All characters of a campaign, players first then alphabetically.
pub fn list_for_campaign<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<Vec<Character>> {
    storage.query_vec(
        // is_player DESC sorts players (1) before NPCs (0) — is_player is
        // stored as 0/1 — and name ASC breaks ties within a group.
        // The campaign filter is bound (?1), so rows never leak across owners.
        "SELECT id, campaign_id, name, is_player, bio, stats, extra, created_at
         FROM characters WHERE campaign_id = ?1
         ORDER BY is_player DESC, name ASC",
        &[&campaign_id],
        map_character,
    )
}

/// Fetch one character by id.
pub fn get<S: Storage + ?Sized>(
    storage: &S,
    character_id: &str,
) -> Result<Option<Character>> {
    // The id filter is bound (?1); query_row yields None when no row matches.
    storage.query_row(
        "SELECT id, campaign_id, name, is_player, bio, stats, extra, created_at
         FROM characters WHERE id = ?1",
        &[&character_id],
        map_character,
    )
}

/// Full replace of editable fields; returns the updated row.
pub fn update<S: Storage + ?Sized>(
    storage: &S,
    character: &Character,
) -> Result<Option<Character>> {
    let stats = character.stats.to_string();
    // UPDATE replaces the editable fields wholesale (no partial merge); the
    // reported row count tells us whether the id actually existed.
    let changed = storage.execute(
        "UPDATE characters
         SET name = ?1, is_player = ?2, bio = ?3, stats = ?4
         WHERE id = ?5",
        &[
            &character.name,
            &(character.is_player as i64),
            &character.bio,
            &stats,
            &character.id,
        ],
    )?;
    if changed == 0 {
        // No row matched (unknown id); report the miss instead of returning
        // a stale read of a row we did not touch.
        return Ok(None);
    }
    // Re-read the row so the caller gets the persisted truth, including any
    // column coercions SQLite applied on write.
    get(storage, &character.id)
}

/// Delete a character; returns whether a row was actually deleted.
pub fn delete<S: Storage + ?Sized>(
    storage: &S,
    character_id: &str,
) -> Result<bool> {
    // Deleting a character leaves the owning campaign untouched; the id filter
    // is bound (?1), so only the intended row can be removed.
    let changed = storage
        .execute("DELETE FROM characters WHERE id = ?1", &[&character_id])?;
    // More than zero changed rows means the character existed and was removed.
    Ok(changed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::storage::Database;
    use roleplayer_core::{new_id, now_rfc3339};

    fn test_character(
        campaign_id: &str,
        name: &str,
        is_player: bool,
    ) -> Character {
        Character {
            // Fresh ids per call so tests never collide on the primary key.
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            name: name.to_string(),
            is_player,
            bio: String::new(),
            stats: serde_json::json!({ "hp": 10 }),
            extra: serde_json::json!({}),
            created_at: now_rfc3339(),
        }
    }

    #[test]
    fn character_crud_and_campaign_scoping() {
        let storage = Database::open_in_memory().expect("in-memory db");

        // Parent rows must exist before characters reference them (FK).
        let seed_campaign = |campaign_id: &str| {
            storage
                .execute(
                    "INSERT INTO campaigns (id, name, description, created_at, updated_at)
                     VALUES (?1, ?2, '', '', '')",
                    &[&campaign_id.to_string(), &campaign_id.to_string()],
                )
                .expect("seed campaign");
        };
        seed_campaign("camp-a");
        seed_campaign("camp-b");

        let mut hero = test_character("camp-a", "Elara", true);
        let npc = test_character("camp-a", "Barkeep", false);
        let other_camp = test_character("camp-b", "Stranger", false);

        insert(&storage, &hero).expect("insert hero");
        insert(&storage, &npc).expect("insert npc");
        insert(&storage, &other_camp).expect("insert other");

        // The cross-campaign row must not appear in camp-a's list.
        let camp_a =
            list_for_campaign(&storage, "camp-a").expect("list camp-a");
        assert_eq!(camp_a.len(), 2);
        // Players sort first.
        assert_eq!(camp_a[0].name, "Elara");

        // Update mutates name and stats; the re-read row reflects both.
        hero.name = "Elara Dawn".to_string();
        hero.stats = serde_json::json!({ "hp": 15 });
        update(&storage, &hero).expect("update");
        let fetched = get(&storage, &hero.id).expect("get").expect("exists");
        assert_eq!(fetched.name, "Elara Dawn");
        assert_eq!(fetched.stats["hp"], 15);

        // Deleting the hero removes it; a follow-up get confirms it is gone.
        assert!(delete(&storage, &hero.id).expect("delete"));
        assert!(get(&storage, &hero.id).expect("get").is_none());
    }
}
