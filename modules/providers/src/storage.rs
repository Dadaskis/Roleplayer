//! SQLite repository for provider configs (the *non-secret* parts).

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::{ProviderConfig, ProviderKind};

fn map_config(row: &Row<'_>) -> rusqlite::Result<ProviderConfig> {
    Ok(ProviderConfig {
        id: row.get("id")?,
        name: row.get("name")?,
        kind: ProviderKind::from_wire(&row.get::<_, String>("provider_kind")?),
        base_url: row.get("base_url")?,
        model: row.get("model")?,
        is_default: row.get::<_, i64>("is_default")? != 0,
        created_at: row.get("created_at")?,
    })
}

/// Upsert a provider config.
pub fn upsert<S: Storage + ?Sized>(
    storage: &S,
    config: &ProviderConfig,
) -> Result<()> {
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
            &config.kind.as_str(),
            &config.base_url,
            &config.model,
            &(config.is_default as i64),
            &config.created_at,
        ],
    )
    .map(|_changed| ())
}

/// All stored provider configs.
pub fn list<S: Storage + ?Sized>(storage: &S) -> Result<Vec<ProviderConfig>> {
    storage.query_vec(
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
    storage.query_row(
        "SELECT id, name, provider_kind, base_url, model, is_default, created_at
         FROM provider_cfg WHERE id = ?1",
        &[&provider_id],
        map_config,
    )
}

/// Clear the `is_default` flag on every row (called before setting a new one).
pub fn clear_defaults<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    storage
        .execute("UPDATE provider_cfg SET is_default = 0", &[])
        .map(|_changed| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::now_rfc3339;
    use roleplayer_core::storage::Database;

    fn config(id: &str, kind: ProviderKind) -> ProviderConfig {
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
        let mut changed = config("p1", ProviderKind::Mock);
        changed.model = "model-y".to_string();
        upsert(&storage, &changed).expect("upsert");

        let fetched = get(&storage, "p1").expect("get").expect("exists");
        assert_eq!(fetched.model, "model-y");
        assert_eq!(list(&storage).expect("list").len(), 1);
    }
}
