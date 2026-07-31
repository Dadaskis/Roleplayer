//! SQLite repository for campaigns.
//!
//! All queries are parameterized — values are bound, never interpolated
//! (§5.16 of AGENTS.md). This layer has no business logic, only row mapping.
//! The functions are generic over `S: Storage` so they work against any backend
//! impl (SQLite file, in-memory test DB, future backends) while keeping the
//! seam object-free (`Storage` has generic methods, so it is not dyn-compatible).

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::Campaign;

fn map_campaign(row: &Row<'_>) -> rusqlite::Result<Campaign> {
    Ok(Campaign {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        ruleset_id: row.get("ruleset_id")?,
        settings: row
            .get::<_, String>("settings")?
            .parse()
            .unwrap_or(serde_json::Value::Object(Default::default())),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// Insert a new campaign row. No-op-free: callers validate first.
pub fn insert<S: Storage + ?Sized>(
    storage: &S,
    campaign: &Campaign,
) -> Result<()> {
    let settings = campaign.settings.to_string();
    storage
        .execute(
            "INSERT INTO campaigns (id, name, description, ruleset_id, settings, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                &campaign.id,
                &campaign.name,
                &campaign.description,
                &campaign.ruleset_id,
                &settings,
                &campaign.created_at,
                &campaign.updated_at,
            ],
        )
        .map(|_changed| ())
}

/// List all campaigns, newest first.
pub fn list<S: Storage + ?Sized>(storage: &S) -> Result<Vec<Campaign>> {
    storage.query_vec(
        "SELECT id, name, description, ruleset_id, settings, created_at, updated_at
         FROM campaigns ORDER BY created_at DESC",
        &[],
        map_campaign,
    )
}

/// Fetch one campaign by id.
pub fn get<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<Option<Campaign>> {
    storage.query_row(
        "SELECT id, name, description, ruleset_id, settings, created_at, updated_at
         FROM campaigns WHERE id = ?1",
        &[&campaign_id],
        map_campaign,
    )
}

/// Full replace of the editable campaign fields; returns the updated row.
pub fn update<S: Storage + ?Sized>(
    storage: &S,
    campaign: &Campaign,
    updated_at: &str,
) -> Result<Option<Campaign>> {
    let settings = campaign.settings.to_string();
    let changed = storage.execute(
        "UPDATE campaigns
         SET name = ?1, description = ?2, ruleset_id = ?3, settings = ?4, updated_at = ?5
         WHERE id = ?6",
        &[
            &campaign.name,
            &campaign.description,
            &campaign.ruleset_id,
            &settings,
            &updated_at,
            &campaign.id,
        ],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    get(storage, &campaign.id)
}

/// Delete a campaign; rows cascade (messages, characters, world state, ...).
/// Returns whether a row was actually deleted.
pub fn delete<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<bool> {
    let changed = storage
        .execute("DELETE FROM campaigns WHERE id = ?1", &[&campaign_id])?;
    Ok(changed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::storage::Database;
    use roleplayer_core::{new_id, now_rfc3339};

    fn test_campaign(name: &str) -> Campaign {
        let now = now_rfc3339();
        Campaign {
            id: new_id(),
            name: name.to_string(),
            description: String::new(),
            ruleset_id: None,
            settings: serde_json::json!({}),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn crud_round_trip() {
        let storage = Database::open_in_memory().expect("in-memory db");
        let mut campaign = test_campaign("Round Trip");

        insert(&storage, &campaign).expect("insert");
        let fetched =
            get(&storage, &campaign.id).expect("get").expect("exists");
        assert_eq!(fetched.name, "Round Trip");

        campaign.name = "Renamed".to_string();
        update(&storage, &campaign, &now_rfc3339()).expect("update");
        let fetched =
            get(&storage, &campaign.id).expect("get").expect("exists");
        assert_eq!(fetched.name, "Renamed");

        assert!(delete(&storage, &campaign.id).expect("delete"));
        assert!(get(&storage, &campaign.id).expect("get").is_none());
        // Deleting an unknown id is not an error, just a no-op.
        assert!(!delete(&storage, &new_id()).expect("delete missing"));
    }

    #[test]
    fn list_orders_newest_first() {
        let storage = Database::open_in_memory().expect("in-memory db");
        let mut first = test_campaign("first");
        first.created_at = "2024-01-01T00:00:00Z".to_string();
        first.updated_at = first.created_at.clone();
        let mut second = test_campaign("second");
        second.created_at = "2024-06-01T00:00:00Z".to_string();
        second.updated_at = second.created_at.clone();

        insert(&storage, &first).expect("insert first");
        insert(&storage, &second).expect("insert second");

        let list = list(&storage).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "second");
        assert_eq!(list[1].name, "first");
    }
}
