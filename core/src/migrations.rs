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
    Migrations::new(vec![M::up(SCHEMA_V1), M::up(SCHEMA_V2), M::up(SCHEMA_V3)])
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
-- `meta` is the version ledger rusqlite_migration reads/writes: it stores
-- the applied schema version so only missing migrations run on next open.
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

-- Speeds the transcript fetch that drives the context builder — a known hot
-- path (§5.12 of AGENTS.md), so the index earns its write cost.
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

-- FTS5 external-content table: the index lives here while the source text
-- stays in `messages`. `content_rowid` joins on the implicit integer
-- `rowid` of the TEXT-keyed messages table (FTS requires an integer key).
CREATE VIRTUAL TABLE messages_fts USING fts5(
    campaign_id, role, content,
    content='messages',
    content_rowid='rowid'
);

-- FTS5 external-content tables have no built-in update path, so every
-- change is expressed as a delete plus an insert; `messages_fts` given as
-- the first column selects the special 'delete' command.
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, campaign_id, role, content)
    VALUES (new.rowid, new.campaign_id, new.role, new.content);
END;

CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, campaign_id, role, content)
    VALUES ('delete', old.rowid, old.campaign_id, old.role, old.content);
END;

CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
    -- 'delete' the old indexed row, then insert the new one in its place.
    INSERT INTO messages_fts(messages_fts, rowid, campaign_id, role, content)
    VALUES ('delete', old.rowid, old.campaign_id, old.role, old.content);
    INSERT INTO messages_fts(rowid, campaign_id, role, content)
    VALUES (new.rowid, new.campaign_id, new.role, new.content);
END;
"#;

/// Version 2 — player message modes (action vs. speech).
///
/// `mode` tells the GM whether a player message is dialogue (`speech`) or
/// narration (`action`). `NOT NULL DEFAULT 'action'` backfills every existing
/// row as an action, and the `CHECK` rejects any other value at the DB
/// boundary (§5.10 of AGENTS.md). The FTS5 index only covers
/// `campaign_id/role/content`, so adding this column needs no trigger change.
const SCHEMA_V2: &str = r#"
ALTER TABLE messages ADD COLUMN mode TEXT NOT NULL DEFAULT 'action'
    CHECK (mode IN ('action', 'speech'));
"#;

/// Version 3 — campaign lifecycle status.
///
/// A campaign starts in `setup` (the GM asks clarifying questions before the
/// world exists), moves to `worldgen` while the GM generates the world and
/// characters (a transient, single-flight state), and finally `active` for
/// normal play. `NOT NULL DEFAULT 'setup'` backfills existing rows as setups;
/// the `CHECK` keeps the state machine closed at the DB boundary (§5.10).
const SCHEMA_V3: &str = r#"
ALTER TABLE campaigns ADD COLUMN status TEXT NOT NULL DEFAULT 'setup'
    CHECK (status IN ('setup', 'worldgen', 'active'));
"#;

#[cfg(test)]
mod tests {
    use super::migrations;
    use super::SCHEMA_V1;
    use super::SCHEMA_V2;
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

    #[test]
    fn v2_adds_message_mode_with_action_default() {
        // Prove the v1 -> v2 upgrade for real: apply ONLY v1, write a row the
        // way v1 shaped it (no `mode` column), then apply v2 and confirm the
        // ALTER TABLE backfills that pre-existing row with 'action'.
        use rusqlite_migration::{Migrations, M};

        // A raw connection lets us apply migrations in two steps instead of
        // running everything to latest in one shot.
        let mut connection =
            rusqlite::Connection::open_in_memory().expect("in-memory conn");
        // Step 1: v1 only — this is the schema a pre-upgrade database has.
        Migrations::new(vec![M::up(SCHEMA_V1)])
            .to_latest(&mut connection)
            .expect("v1 applies");
        // Seed a campaign, then a message row exactly as v1 shaped it.
        connection
            .execute(
                "INSERT INTO campaigns (id, name, description, created_at, updated_at)
                 VALUES ('c1', 'Camp', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed campaign");
        connection
            .execute(
                "INSERT INTO messages (id, campaign_id, role, content, model, turn_index, created_at)
                 VALUES ('m1', 'c1', 'user', '[]', NULL, 1, '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed v1 message");

        // Step 2: the real upgrade path (v1 -> v2) over the existing rows.
        migrations().to_latest(&mut connection).expect("v2 applies over v1");

        // The DEFAULT must have backfilled the v1 row; a NULL here would mean
        // the migration (not the test) is broken.
        let mode: String = connection
            .query_row("SELECT mode FROM messages WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .expect("mode column exists after v2");
        assert_eq!(mode, "action");
    }

    #[test]
    fn v3_adds_campaign_status_with_setup_default() {
        // Prove the v2 -> v3 upgrade: a campaign row written under v2 (no
        // `status` column) reads back as 'setup' once v3 is applied.
        use rusqlite_migration::{Migrations, M};

        let mut connection =
            rusqlite::Connection::open_in_memory().expect("in-memory conn");
        // v2 only — the schema a pre-upgrade database has.
        Migrations::new(vec![M::up(SCHEMA_V1), M::up(SCHEMA_V2)])
            .to_latest(&mut connection)
            .expect("v1+v2 apply");
        connection
            .execute(
                "INSERT INTO campaigns (id, name, description, ruleset_id, settings, created_at, updated_at)
                 VALUES ('c1', 'Camp', '', NULL, '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed v2 campaign");

        // The real upgrade path (v1+v2 -> v3) over the existing row.
        migrations().to_latest(&mut connection).expect("v3 applies over v2");

        // The DEFAULT must have backfilled the v2 row as a setup campaign.
        let status: String = connection
            .query_row(
                "SELECT status FROM campaigns WHERE id = 'c1'",
                [],
                |row| row.get(0),
            )
            .expect("status column exists after v3");
        assert_eq!(status, "setup");
    }
}
