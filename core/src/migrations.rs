//! Versioned schema migrations (§5.4, §5.16 of AGENTS.md).
//!
//! Migrations are up-only and never edited once merged. The whole history is
//! defined here as one list; `rusqlite_migration` records the applied version
//! in a `meta` table and applies only what is missing.

use rusqlite_migration::{Migrations, M};

/// Build the full ordered migration list.
///
/// Not a `const` because `Migrations::new(vec![...])` cannot run in const
/// context (heap allocation). Called once at database open; appending a new
/// migration is adding one entry at the END — never editing or reordering.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(SCHEMA_V1)])
}

/// Version 1 — the initial schema.
///
/// Notes on shape choices:
/// - Every entity has a TEXT UUID primary key (backend-generated, §5.4).
/// - Anything that can vary ("any kind of data") lives in JSON columns first
///   and is only promoted to a typed column when justified (§5.4).
/// - `state_changes` is the anti-hallucination audit trail (§4.6 of PLAN.md).
/// - `messages_fts` keeps a full-text index over the transcript for search.
const SCHEMA_V1: &str = r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE campaigns (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    ruleset_id  TEXT,
    settings    TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE rulesets (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    world_rules   TEXT NOT NULL DEFAULT '{}',
    is_builtin    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);

CREATE TABLE characters (
    id          TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    is_player   INTEGER NOT NULL DEFAULT 0,
    bio         TEXT NOT NULL DEFAULT '',
    stats       TEXT NOT NULL DEFAULT '{}',
    extra       TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL
);

CREATE TABLE messages (
    id          TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    model       TEXT,
    turn_index  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_messages_campaign ON messages(campaign_id, turn_index);

CREATE TABLE world_state (
    campaign_id TEXT PRIMARY KEY REFERENCES campaigns(id) ON DELETE CASCADE,
    document    TEXT NOT NULL DEFAULT '{}',
    updated_at  TEXT NOT NULL
);

CREATE TABLE state_changes (
    id           TEXT PRIMARY KEY,
    campaign_id  TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    tool         TEXT NOT NULL,
    args         TEXT NOT NULL DEFAULT '{}',
    before_value TEXT,
    after_value  TEXT,
    message_id   TEXT,
    created_at   TEXT NOT NULL
);

CREATE TABLE memories (
    id          TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    summary     TEXT NOT NULL,
    source_from INTEGER NOT NULL,
    source_to   INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE provider_cfg (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    provider_kind TEXT NOT NULL,
    base_url      TEXT NOT NULL DEFAULT '',
    model         TEXT NOT NULL,
    extra         TEXT NOT NULL DEFAULT '{}',
    is_default    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    campaign_id, role, content,
    content='messages',
    content_rowid='rowid'
);

CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, campaign_id, role, content)
    VALUES (new.rowid, new.campaign_id, new.role, new.content);
END;

CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, campaign_id, role, content)
    VALUES ('delete', old.rowid, old.campaign_id, old.role, old.content);
END;

CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, campaign_id, role, content)
    VALUES ('delete', old.rowid, old.campaign_id, old.role, old.content);
    INSERT INTO messages_fts(rowid, campaign_id, role, content)
    VALUES (new.rowid, new.campaign_id, new.role, new.content);
END;
"#;

#[cfg(test)]
mod tests {
    use crate::storage::{Database, Storage};

    #[test]
    fn migrations_apply_cleanly_and_schema_is_queryable() {
        // Headless verification (§5.11): the full schema must apply in-memory.
        let database =
            Database::open_in_memory().expect("in-memory db should open");

        // Insert a campaign and read it back to prove the tables exist.
        database
            .execute(
                "INSERT INTO campaigns (id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, '', '', '')",
                &[&"c1".to_string(), &"Test Campaign".to_string()],
            )
            .expect("insert should work");

        let name: Option<String> = database
            .query_row(
                "SELECT name FROM campaigns WHERE id = ?1",
                &[&"c1".to_string()],
                |row| row.get(0),
            )
            .expect("query should work");

        assert_eq!(name.as_deref(), Some("Test Campaign"));
    }
}
