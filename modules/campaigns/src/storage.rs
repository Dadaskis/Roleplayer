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

use crate::domain::{Campaign, CampaignStatus};

fn map_campaign(row: &Row<'_>) -> rusqlite::Result<Campaign> {
    // Translate one SQLite row into a domain entity; every column is fetched
    // by name (not index) so adding columns later does not shift offsets.
    Ok(Campaign {
        // Column names mirror the domain fields, keeping the mapping obvious.
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        // Optional FK: NULL in SQL maps to None (no linked ruleset).
        ruleset_id: row.get("ruleset_id")?,
        // The lifecycle status wire string; unknown values degrade to Setup.
        status: CampaignStatus::from_wire(&row.get::<_, String>("status")?),
        // `settings` is stored as JSON text; parse it back, degrading to an
        // empty object if a row is corrupt so the UI never crashes (§5.10).
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
    // JSON values are stored as text in SQLite; serialize once for the bind.
    let settings = campaign.settings.to_string();
    // Placeholders ?1..?8 bind in column order; binding, never string-building,
    // keeps user data out of the SQL text (§5.16).
    storage
        .execute(
            "INSERT INTO campaigns (id, name, description, ruleset_id, status, settings, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                // The id and timestamps come from the backend caller, which the
                // app crate fills in — never from the client (§5.4).
                &campaign.id,
                &campaign.name,
                &campaign.description,
                // Option<String> binds as NULL when absent; the FK stays open.
                &campaign.ruleset_id,
                // Status is backend-set only; every new campaign is a setup.
                &campaign.status.as_str(),
                &settings,
                &campaign.created_at,
                &campaign.updated_at,
            ],
        )
        // Row count is uninteresting for an insert; map to unit.
        .map(|_changed| ())
}

/// Set a campaign's lifecycle status (backend-driven state transition).
/// Returns whether a row was actually updated.
pub fn set_status<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    status: CampaignStatus,
    updated_at: &str,
) -> Result<bool> {
    // Only the status and its timestamp change; the editable fields are
    // untouched so a transition never clobbers user data.
    let changed = storage.execute(
        "UPDATE campaigns SET status = ?1, updated_at = ?2 WHERE id = ?3",
        &[&status.as_str(), &updated_at, &campaign_id],
    )?;
    // More than zero changed rows means the campaign existed.
    Ok(changed > 0)
}

/// List all campaigns, newest first.
pub fn list<S: Storage + ?Sized>(storage: &S) -> Result<Vec<Campaign>> {
    storage.query_vec(
        // The full column set matches map_campaign's by-name fetches; DESC on
        // created_at surfaces the newest campaign at index 0 for the sidebar.
        "SELECT id, name, description, ruleset_id, status, settings, created_at, updated_at
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
    // The id filter is bound (?1); query_row yields None when no row matches.
    storage.query_row(
        "SELECT id, name, description, ruleset_id, status, settings, created_at, updated_at
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
    // UPDATE replaces the editable fields wholesale (no partial merge); the
    // reported row count tells us whether the id actually existed.
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
        // No row matched (unknown id); report the miss instead of returning
        // a stale read of a row we did not touch.
        return Ok(None);
    }
    // Re-read the row so the caller gets the persisted truth, including any
    // column coercions SQLite applied on write.
    get(storage, &campaign.id)
}

/// Delete a campaign; rows cascade (messages, characters, world state, ...).
/// Returns whether a row was actually deleted.
pub fn delete<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<bool> {
    // Foreign keys cascade on the campaigns parent row, so removing it cleans
    // up children (messages, characters, world state, ...) in one statement.
    let changed = storage
        .execute("DELETE FROM campaigns WHERE id = ?1", &[&campaign_id])?;
    // More than zero changed rows means the campaign existed and was removed.
    Ok(changed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::storage::Database;
    use roleplayer_core::{new_id, now_rfc3339};

    fn test_campaign(name: &str) -> Campaign {
        // One shared timestamp so created_at equals updated_at on a fresh row.
        let now = now_rfc3339();
        Campaign {
            id: new_id(),
            name: name.to_string(),
            description: String::new(),
            ruleset_id: None,
            // Every test campaign starts in the default setup phase.
            status: CampaignStatus::Setup,
            settings: serde_json::json!({}),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn crud_round_trip() {
        // In-memory SQLite gives the full schema with zero setup.
        let storage = Database::open_in_memory().expect("in-memory db");
        let mut campaign = test_campaign("Round Trip");

        insert(&storage, &campaign).expect("insert");
        let fetched =
            get(&storage, &campaign.id).expect("get").expect("exists");
        assert_eq!(fetched.name, "Round Trip");

        // Mutate and write back; update returns the re-read row.
        campaign.name = "Renamed".to_string();
        update(&storage, &campaign, &now_rfc3339()).expect("update");
        let fetched =
            get(&storage, &campaign.id).expect("get").expect("exists");
        assert_eq!(fetched.name, "Renamed");

        // Delete removes the row; a follow-up get confirms it is gone.
        assert!(delete(&storage, &campaign.id).expect("delete"));
        assert!(get(&storage, &campaign.id).expect("get").is_none());
        // Deleting an unknown id is not an error, just a no-op.
        assert!(!delete(&storage, &new_id()).expect("delete missing"));
    }

    #[test]
    fn list_orders_newest_first() {
        let storage = Database::open_in_memory().expect("in-memory db");
        // Pin explicit timestamps so ordering is not dependent on wall-clock
        // time racing between the two inserts.
        let mut first = test_campaign("first");
        first.created_at = "2024-01-01T00:00:00Z".to_string();
        first.updated_at = first.created_at.clone();
        let mut second = test_campaign("second");
        second.created_at = "2024-06-01T00:00:00Z".to_string();
        second.updated_at = second.created_at.clone();

        insert(&storage, &first).expect("insert first");
        insert(&storage, &second).expect("insert second");

        // The newer (June) row must lead the list.
        let list = list(&storage).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "second");
        assert_eq!(list[1].name, "first");
    }
}
