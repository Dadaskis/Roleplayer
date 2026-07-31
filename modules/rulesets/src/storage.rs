//! SQLite repository for rulesets. Parameterized queries only (§5.16).

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::Ruleset;

fn map_ruleset(row: &Row<'_>) -> rusqlite::Result<Ruleset> {
    // Translate one SQLite row into a domain entity; columns are fetched by
    // name so the mapping survives column reordering.
    Ok(Ruleset {
        id: row.get("id")?,
        name: row.get("name")?,
        system_prompt: row.get("system_prompt")?,
        // world_rules is JSON text; degrade a corrupt value to `{}` rather
        // than failing the whole read (§5.10).
        world_rules: row
            .get::<_, String>("world_rules")?
            .parse()
            .unwrap_or(serde_json::Value::Object(Default::default())),
        // SQLite has no boolean type; is_builtin is stored as 0/1 integer.
        is_builtin: row.get::<_, i64>("is_builtin")? != 0,
        created_at: row.get("created_at")?,
    })
}

/// Insert a ruleset row.
pub fn insert<S: Storage + ?Sized>(
    storage: &S,
    ruleset: &Ruleset,
) -> Result<()> {
    let world_rules = ruleset.world_rules.to_string();
    // Placeholders ?1..?6 bind in column order; binding, never string-building,
    // keeps user data out of the SQL text (§5.16).
    storage
        .execute(
            "INSERT INTO rulesets (id, name, system_prompt, world_rules, is_builtin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                &ruleset.id,
                &ruleset.name,
                &ruleset.system_prompt,
                &world_rules,
                // The bool is widened to an integer column; 1 = built-in preset.
                &(ruleset.is_builtin as i64),
                &ruleset.created_at,
            ],
        )
        // Row count is uninteresting for an insert; map to unit.
        .map(|_changed| ())
}

/// List all rulesets, built-ins first then newest.
pub fn list<S: Storage + ?Sized>(storage: &S) -> Result<Vec<Ruleset>> {
    storage.query_vec(
        // is_builtin DESC pins seeded presets on top; created_at DESC puts
        // the newest custom ruleset first among the rest.
        // The full column set matches map_ruleset's by-name fetches.
        "SELECT id, name, system_prompt, world_rules, is_builtin, created_at
         FROM rulesets ORDER BY is_builtin DESC, created_at DESC",
        &[],
        map_ruleset,
    )
}

/// Fetch one ruleset by id.
pub fn get<S: Storage + ?Sized>(
    storage: &S,
    ruleset_id: &str,
) -> Result<Option<Ruleset>> {
    // The id filter is bound (?1); query_row yields None when no row matches.
    storage.query_row(
        "SELECT id, name, system_prompt, world_rules, is_builtin, created_at
         FROM rulesets WHERE id = ?1",
        &[&ruleset_id],
        map_ruleset,
    )
}

/// Full replace of editable fields; returns the updated row.
pub fn update<S: Storage + ?Sized>(
    storage: &S,
    ruleset: &Ruleset,
) -> Result<Option<Ruleset>> {
    let world_rules = ruleset.world_rules.to_string();
    // The is_builtin = 0 guard lives in the SQL, not the service, so the DB
    // enforces built-in protection even if a future caller forgets to check.
    let changed = storage.execute(
        "UPDATE rulesets
         SET name = ?1, system_prompt = ?2, world_rules = ?3
         WHERE id = ?4 AND is_builtin = 0",
        &[&ruleset.name, &ruleset.system_prompt, &world_rules, &ruleset.id],
    )?;
    if changed == 0 {
        // Either the id is unknown or it is a protected built-in.
        return Ok(None);
    }
    // Re-read the row so the caller gets the persisted truth, including any
    // column coercions SQLite applied on write.
    get(storage, &ruleset.id)
}

/// Delete a ruleset; built-ins are protected. Returns whether a row was deleted.
pub fn delete<S: Storage + ?Sized>(
    storage: &S,
    ruleset_id: &str,
) -> Result<bool> {
    // Guard in SQL keeps built-in presets immutable at the storage layer,
    // so protection holds even if a future service forgets to check.
    // The is_builtin = 0 guard in SQL makes the delete a no-op for presets;
    // changed == 0 either means the id is unknown or it is protected.
    let changed = storage.execute(
        "DELETE FROM rulesets WHERE id = ?1 AND is_builtin = 0",
        &[&ruleset_id],
    )?;
    // More than zero changed rows means a custom ruleset was actually removed.
    Ok(changed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::storage::Database;
    use roleplayer_core::{new_id, now_rfc3339};

    fn test_ruleset(name: &str, builtin: bool) -> Ruleset {
        Ruleset {
            id: new_id(),
            name: name.to_string(),
            system_prompt: "You are the GM.".to_string(),
            world_rules: serde_json::json!({ "magic": "rare" }),
            is_builtin: builtin,
            created_at: now_rfc3339(),
        }
    }

    #[test]
    fn builtin_rulesets_are_protected() {
        let storage = Database::open_in_memory().expect("in-memory db");
        let builtin = test_ruleset("Builtin", true);
        let custom = test_ruleset("Custom", false);

        insert(&storage, &builtin).expect("insert builtin");
        insert(&storage, &custom).expect("insert custom");

        // Built-ins cannot be updated or deleted.
        let mut mutated = builtin.clone();
        mutated.name = "Hacked".to_string();
        assert!(update(&storage, &mutated).expect("update builtin").is_none());
        assert!(!delete(&storage, &builtin.id).expect("delete builtin"));

        // Custom rulesets can.
        assert!(delete(&storage, &custom.id).expect("delete custom"));
    }
}
