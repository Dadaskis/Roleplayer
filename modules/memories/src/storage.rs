//! SQLite repository for memories. Parameterized queries only (§5.16).

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::Memory;

fn map_memory(row: &Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: row.get("id")?,
        campaign_id: row.get("campaign_id")?,
        summary: row.get("summary")?,
        source_from: row.get("source_from")?,
        source_to: row.get("source_to")?,
        created_at: row.get("created_at")?,
    })
}

/// Insert a memory row.
pub fn insert<S: Storage + ?Sized>(storage: &S, memory: &Memory) -> Result<()> {
    // Placeholders ?1..?6 bind in column order; binding, never string-building,
    // keeps user data out of the SQL text (§5.16).
    storage
        .execute(
            "INSERT INTO memories (id, campaign_id, summary, source_from, source_to, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                &memory.id,
                &memory.campaign_id,
                &memory.summary,
                &memory.source_from,
                &memory.source_to,
                &memory.created_at,
            ],
        )
        // Row count is uninteresting for an insert; map to unit.
        .map(|_changed| ())
}

/// Memories of a campaign, newest first.
pub fn list_for_campaign<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<Vec<Memory>> {
    storage.query_vec(
        // Newest memory first; the campaign filter is bound (?1) so memories
        // never leak across campaigns.
        "SELECT id, campaign_id, summary, source_from, source_to, created_at
         FROM memories WHERE campaign_id = ?1
         ORDER BY created_at DESC",
        &[&campaign_id],
        map_memory,
    )
}

/// Delete a memory; returns whether a row was actually deleted.
pub fn delete<S: Storage + ?Sized>(
    storage: &S,
    memory_id: &str,
) -> Result<bool> {
    // Only the targeted memory is removed; the campaign row is untouched.
    let changed =
        storage.execute("DELETE FROM memories WHERE id = ?1", &[&memory_id])?;
    // More than zero changed rows means the memory existed and was removed.
    Ok(changed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::storage::Database;
    use roleplayer_core::{new_id, now_rfc3339};

    fn seed_campaign(storage: &Database, campaign_id: &str) {
        // The memories FK requires a parent campaign row; seed one with the
        // minimal columns the schema accepts.
        storage
            .execute(
                "INSERT INTO campaigns (id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, '', '', '')",
                &[&campaign_id.to_string(), &campaign_id.to_string()],
            )
            .expect("seed campaign");
    }

    fn test_memory(campaign_id: &str, summary: &str) -> Memory {
        Memory {
            // Fresh ids per call so tests never collide on the primary key.
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            summary: summary.to_string(),
            source_from: 1,
            source_to: 5,
            created_at: now_rfc3339(),
        }
    }

    #[test]
    fn memories_crud_per_campaign() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");

        insert(&storage, &test_memory("camp-1", "met the innkeeper"))
            .expect("insert");
        insert(&storage, &test_memory("camp-1", "cleared the cellar"))
            .expect("insert");

        // Both memories come back, scoped to this campaign.
        let memories = list_for_campaign(&storage, "camp-1").expect("list");
        assert_eq!(memories.len(), 2);

        // Removing one leaves the other in place.
        assert!(delete(&storage, &memories[0].id).expect("delete"));
        assert_eq!(
            list_for_campaign(&storage, "camp-1").expect("list").len(),
            1
        );
    }
}
