//! SQLite repository for the world-state document and its audit trail.
//!
//! All reads/writes are parameterized (§5.16). Mutations run inside a short
//! transaction (single-writer discipline) so the document and its audit row
//! commit atomically — a partial write here would corrupt the source of truth.

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use roleplayer_core::{new_id, now_rfc3339};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::StateChange;

/// The current world-state document for a campaign; `{}` when never set.
pub fn get_document<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<serde_json::Value> {
    storage
        .query_row(
            "SELECT document FROM world_state WHERE campaign_id = ?1",
            &[&campaign_id],
            // Fetch as raw text first; the JSON parse happens in the map below.
            |row| row.get::<_, String>(0),
        )
        // The document is JSON text; parse it, treating an absent row or a
        // corrupt payload as an empty object so reads never fail (§5.10).
        .map(|document| {
            document.and_then(|raw| raw.parse().ok()).unwrap_or_else(|| {
                serde_json::Value::Object(Default::default())
            })
        })
}

/// Set one key in the document inside a transaction, recording the audit row.
///
/// Returns `(before, after)` snapshots. Reads and writes run on the same
/// connection (the transaction), so there is no read-modify-write race.
pub fn set_key<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    key: &str,
    value: &serde_json::Value,
    tool: &str,
    args: &serde_json::Value,
    message_id: Option<&str>,
) -> Result<(serde_json::Value, serde_json::Value)> {
    storage.with_transaction(|connection| {
        // Every read/write below shares this one transaction connection, so the
        // read-modify-write cannot race another writer (single-writer §5.16).
        let mut document = load_document(connection, campaign_id)?;
        // Read the key's current value BEFORE mutating, so the audit trail can
        // record what the change replaced (anti-hallucination, §4.6).
        // A missing key snapshots as Null (the canonical "did not exist").
        let before_value =
            document.get(key).cloned().unwrap_or(serde_json::Value::Null);

        // The document is expected to be a JSON object; refuse (typed error)
        // instead of panicking if a corrupt row holds something else.
        document
            .as_object_mut()
            .ok_or_else(|| {
                roleplayer_core::errors::AppError::Domain(
                    "world state document is not an object".to_string(),
                )
            })?
            .insert(key.to_string(), value.clone());
        // The after-snapshot is exactly what was just written; both sides of
        // the before/after pair are captured for the audit row.
        let after_value = value.clone();

        // Persist the mutated document and the audit row in the SAME
        // transaction so a failure rolls both back (no phantom history).
        persist_document(connection, campaign_id, &document)?;
        record_change(
            connection,
            campaign_id,
            tool,
            args,
            &before_value,
            &after_value,
            message_id,
        )?;
        // Hand the caller both snapshots; the service logs/echoes them.
        Ok((before_value, after_value))
    })
}

/// Remove one key from the document, recording the audit row.
///
/// Returns `(before, after)`; `after` is `Null` and `before` is the removed
/// value (or `Null` if the key did not exist).
pub fn remove_key<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    key: &str,
    tool: &str,
    args: &serde_json::Value,
    message_id: Option<&str>,
) -> Result<(serde_json::Value, serde_json::Value)> {
    storage.with_transaction(|connection| {
        // Same transactional discipline as set_key: one connection owns the
        // whole read-modify-write, so no other writer can interleave.
        let mut document = load_document(connection, campaign_id)?;
        // Same read-before-mutate dance as set_key: capture what we're about
        // to delete for the audit trail.
        let before_value =
            document.get(key).cloned().unwrap_or(serde_json::Value::Null);

        // remove() returns the displaced value (or None if the key was never
        // set); the object check protects against a non-object corrupt row.
        let removed = document
            .as_object_mut()
            .ok_or_else(|| {
                roleplayer_core::errors::AppError::Domain(
                    "world state document is not an object".to_string(),
                )
            })?
            .remove(key);

        let after_value = serde_json::Value::Null;
        // Removal always yields Null after; the removed value is returned for
        // the audit trail, or Null when the key did not exist.
        persist_document(connection, campaign_id, &document)?;
        record_change(
            connection,
            campaign_id,
            tool,
            args,
            &before_value,
            &after_value,
            message_id,
        )?;
        // before is what the key held (or Null); removed is the actual value
        // that left the document, which the caller reports for the audit trail.
        Ok((before_value, removed.unwrap_or(serde_json::Value::Null)))
    })
}

/// Insert one audit row directly (used when a mutation was already applied).
pub fn insert_change<S: Storage + ?Sized>(
    storage: &S,
    change: &StateChange,
) -> Result<()> {
    let args = change.args.to_string();
    let before = change.before_value.to_string();
    let after = change.after_value.to_string();
    // Placeholders ?1..?8 bind in column order; binding, never string-building,
    // keeps user data out of the SQL text (§5.16).
    storage
        .execute(
            "INSERT INTO state_changes
                (id, campaign_id, tool, args, before_value, after_value, message_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                &change.id,
                &change.campaign_id,
                &change.tool,
                &args,
                &before,
                &after,
                // Option<String> binds as NULL when no message triggered it.
                &change.message_id,
                &change.created_at,
            ],
        )
        // Row count is uninteresting for an insert; map to unit.
        .map(|_changed| ())
}

/// Recent audit entries for a campaign, newest first.
pub fn list_changes<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    limit: i64,
) -> Result<Vec<StateChange>> {
    storage.query_vec(
        // Newest-first audit history; LIMIT is bound so a hostile caller cannot
        // ask for an unbounded result set.
        "SELECT id, campaign_id, tool, args, before_value, after_value, message_id, created_at
         FROM state_changes WHERE campaign_id = ?1
         ORDER BY created_at DESC LIMIT ?2",
        &[&campaign_id, &limit],
        map_change,
    )
}

fn map_change(row: &Row<'_>) -> rusqlite::Result<StateChange> {
    // Translate one audit row into a domain entity; the three JSON columns all
    // go through the shared tolerant parse so one corrupt snapshot does not
    // sink the whole history read.
    Ok(StateChange {
        id: row.get("id")?,
        campaign_id: row.get("campaign_id")?,
        tool: row.get("tool")?,
        args: parse_json_or_default(&row.get::<_, String>("args")?),
        before_value: parse_json_or_default(
            &row.get::<_, String>("before_value")?,
        ),
        after_value: parse_json_or_default(
            &row.get::<_, String>("after_value")?,
        ),
        message_id: row.get("message_id")?,
        created_at: row.get("created_at")?,
    })
}

/// Parse a stored JSON column, falling back to `{}` on malformed data so a
/// corrupt row degrades instead of crashing the UI (§5.10).
fn parse_json_or_default(raw: &str) -> serde_json::Value {
    // Parse the stored JSON text; on malformed data yield `{}` so a single
    // corrupt row degrades instead of failing the whole audit read (§5.10).
    raw.parse()
        .unwrap_or_else(|_| serde_json::Value::Object(Default::default()))
}

/// Read the document (or a fresh `{}`) inside a transaction.
fn load_document(
    connection: &Connection,
    campaign_id: &str,
) -> Result<serde_json::Value> {
    // Reads through the transaction's connection, so this load sees whatever
    // this transaction has already written (read-your-writes within a tx).
    let raw: Option<String> = connection
        .query_row(
            "SELECT document FROM world_state WHERE campaign_id = ?1",
            params![campaign_id],
            |row| row.get(0),
        )
        // `.optional()` maps "no such row" (QueryReturnedNoRows) to None.
        .optional()?;
    // Absent row or malformed JSON both degrade to an empty document (§5.10).
    Ok(raw
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| serde_json::Value::Object(Default::default())))
}

fn persist_document(
    connection: &Connection,
    campaign_id: &str,
    document: &serde_json::Value,
) -> Result<()> {
    let raw = document.to_string();
    let updated_at = now_rfc3339();
    // Upsert so the first write creates the row and later writes overwrite it
    // in one statement — no separate exists-check, and it stays inside the
    // caller's transaction (single-writer discipline, §5.16).
    connection
        .execute(
            "INSERT INTO world_state (campaign_id, document, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (campaign_id) DO UPDATE SET document = excluded.document,
                                                      updated_at = excluded.updated_at",
            params![campaign_id, raw, updated_at],
        )
        // The changed-row count is irrelevant to the caller; map to unit.
        .map(|_changed| ())
        // Translate the low-level rusqlite error into the shared taxonomy so it
        // never escapes this seam as a provider-specific type (§5.3).
        .map_err(roleplayer_core::errors::AppError::from)
}

/// Build an audit `StateChange` with a fresh id/timestamp and insert it.
fn record_change(
    connection: &Connection,
    campaign_id: &str,
    tool: &str,
    args: &serde_json::Value,
    before: &serde_json::Value,
    after: &serde_json::Value,
    message_id: Option<&str>,
) -> Result<()> {
    // Build a fresh audit entry with a new id + timestamp; the before/after
    // snapshots were captured by the caller around the mutation.
    let change = StateChange {
        id: new_id(),
        campaign_id: campaign_id.to_string(),
        tool: tool.to_string(),
        args: args.clone(),
        before_value: before.clone(),
        after_value: after.clone(),
        message_id: message_id.map(|value| value.to_string()),
        created_at: now_rfc3339(),
    };
    insert_change_on(connection, &change)
}

/// Insert an audit row on a raw connection (shared by both write paths).
fn insert_change_on(
    connection: &Connection,
    change: &StateChange,
) -> Result<()> {
    connection
        .execute(
            "INSERT INTO state_changes
                (id, campaign_id, tool, args, before_value, after_value, message_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                change.id,
                change.campaign_id,
                change.tool,
                // The JSON snapshots are serialized inline at bind time.
                change.args.to_string(),
                change.before_value.to_string(),
                change.after_value.to_string(),
                change.message_id,
                change.created_at
            ],
        )
        // Same unit-mapping and error-translation as persist_document.
        .map(|_changed| ())
        .map_err(roleplayer_core::errors::AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::storage::Database;

    fn seed_campaign(storage: &Database, campaign_id: &str) {
        // The world_state FK requires a parent campaign row; seed one with
        // the minimal columns the schema accepts.
        storage
            .execute(
                "INSERT INTO campaigns (id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, '', '', '')",
                &[&campaign_id.to_string(), &campaign_id.to_string()],
            )
            .expect("seed campaign");
    }

    #[test]
    fn set_key_records_before_and_after() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");

        // First write: the key did not exist, so before must be Null.
        let (before, after) = set_key(
            &storage,
            "camp-1",
            "room",
            &serde_json::json!("burning"),
            "update_world",
            &serde_json::json!({ "key": "room", "value": "burning" }),
            None,
        )
        .expect("set key");
        assert_eq!(before, serde_json::Value::Null);
        assert_eq!(after, serde_json::json!("burning"));

        // The audit trail captured the change.
        let changes =
            list_changes(&storage, "camp-1", 10).expect("list changes");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].tool, "update_world");
        assert_eq!(changes[0].after_value, serde_json::json!("burning"));

        // The document reflects the new value, and updates record the old value.
        let (before, after) = set_key(
            &storage,
            "camp-1",
            "room",
            &serde_json::json!("flooded"),
            "update_world",
            &serde_json::json!({}),
            None,
        )
        .expect("set key again");
        // The second write's before is the first write's value.
        assert_eq!(before, serde_json::json!("burning"));
        assert_eq!(after, serde_json::json!("flooded"));

        // The stored document reflects the newest value.
        let document = get_document(&storage, "camp-1").expect("get document");
        assert_eq!(document["room"], serde_json::json!("flooded"));
    }

    #[test]
    fn remove_key_records_deletion() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");

        set_key(
            &storage,
            "camp-1",
            "king",
            &serde_json::json!("alive"),
            "update_world",
            &serde_json::json!({}),
            None,
        )
        .expect("set key");

        let (before, removed) = remove_key(
            &storage,
            "camp-1",
            "king",
            "update_world",
            &serde_json::json!({}),
            None,
        )
        .expect("remove key");
        // Removal returns the displaced value as both before and removed.
        assert_eq!(before, serde_json::json!("alive"));
        assert_eq!(removed, serde_json::json!("alive"));

        // The document no longer contains the key at all.
        let document = get_document(&storage, "camp-1").expect("get document");
        assert!(document.get("king").is_none());
    }
}
