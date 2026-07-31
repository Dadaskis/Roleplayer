//! SQLite repository for provider configs (the *non-secret* parts).

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::{ProviderConfig, ProviderKind};

fn map_config(row: &Row<'_>) -> rusqlite::Result<ProviderConfig> {
    // Translate one SQLite row into a domain entity; columns are fetched by
    // name so the mapping survives column reordering.
    Ok(ProviderConfig {
        id: row.get("id")?,
        name: row.get("name")?,
        // provider_kind is stored as a wire string; from_wire falls back to
        // Mock on unknown values so a corrupt row still loads (§5.10).
        kind: ProviderKind::from_wire(&row.get::<_, String>("provider_kind")?),
        base_url: row.get("base_url")?,
        model: row.get("model")?,
        // SQLite has no booleans; is_default is a 0/1 integer.
        is_default: row.get::<_, i64>("is_default")? != 0,
        created_at: row.get("created_at")?,
    })
}

/// Upsert a provider config.
pub fn upsert<S: Storage + ?Sized>(
    storage: &S,
    config: &ProviderConfig,
) -> Result<()> {
    // One statement covers both first-save and update; created_at is
    // deliberately left out of the UPDATE so the original creation time
    // survives re-saves of the same config.
    storage.execute(
        "INSERT INTO provider_cfg (id, name, provider_kind, base_url, model, is_default, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (id) DO UPDATE SET
             name = excluded.name,
             provider_kind = excluded.provider_kind,
             base_url = excluded.base_url,
             model = excluded.model,
             is_default = excluded.is_default",
        &[
            &config.id,
            &config.name,
            // The enum travels as its wire string, matching map_config.
            &config.kind.as_str(),
            &config.base_url,
            &config.model,
            &(config.is_default as i64),
            &config.created_at,
        ],
    )
    // Row count is uninteresting for an upsert; map to unit.
    .map(|_changed| ())
}

/// All stored provider configs.
pub fn list<S: Storage + ?Sized>(storage: &S) -> Result<Vec<ProviderConfig>> {
    storage.query_vec(
        // The full column set matches map_config's by-name fetches; ASC keeps
        // the settings screen in insertion order (oldest first).
        "SELECT id, name, provider_kind, base_url, model, is_default, created_at
         FROM provider_cfg ORDER BY created_at ASC",
        &[],
        map_config,
    )
}

/// One stored provider config by id.
pub fn get<S: Storage + ?Sized>(
    storage: &S,
    provider_id: &str,
) -> Result<Option<ProviderConfig>> {
    // The id filter is bound (?1); query_row yields None when no row matches.
    storage.query_row(
        "SELECT id, name, provider_kind, base_url, model, is_default, created_at
         FROM provider_cfg WHERE id = ?1",
        &[&provider_id],
        map_config,
    )
}

/// Clear the `is_default` flag on every row (called before setting a new one).
pub fn clear_defaults<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    // Single UPDATE resets all flags atomically; avoids the partial state of
    // clearing rows one by one if a later call fails (§5.16).
    // No WHERE clause on purpose: every stored config is demoted in one step.
    storage
        .execute("UPDATE provider_cfg SET is_default = 0", &[])
        // Row count is uninteresting for a blanket reset; map to unit.
        .map(|_changed| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::now_rfc3339;
    use roleplayer_core::storage::Database;

    fn config(id: &str, kind: ProviderKind) -> ProviderConfig {
        // The caller passes an explicit id so tests can refer to the row later.
        ProviderConfig {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            base_url: String::new(),
            model: "model-x".to_string(),
            is_default: false,
            created_at: now_rfc3339(),
        }
    }

    #[test]
    fn upsert_replaces_existing_row() {
        let storage = Database::open_in_memory().expect("in-memory db");
        upsert(&storage, &config("p1", ProviderKind::Mock)).expect("insert");
        // Re-save the same id with a different model; the upsert must replace
        // the existing row rather than duplicating it.
        let mut changed = config("p1", ProviderKind::Mock);
        changed.model = "model-y".to_string();
        upsert(&storage, &changed).expect("upsert");

        let fetched = get(&storage, "p1").expect("get").expect("exists");
        assert_eq!(fetched.model, "model-y");
        assert_eq!(list(&storage).expect("list").len(), 1);
    }
}
