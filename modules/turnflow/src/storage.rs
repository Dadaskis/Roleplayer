//! SQLite repository for the turn transcript (`messages` table).
//!
//! Content is stored as a JSON array of [`ContentBlock`]s so the schema stays
//! stable while message shapes grow (§5.4). All queries are parameterized.

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::service::MessageDto;

/// Insert one transcript row.
pub fn insert_message<S: Storage + ?Sized>(
    storage: &S,
    message: &MessageDto,
) -> Result<()> {
    // Content is a Vec<ContentBlock>; serialize to JSON text for the column
    // and map serialization failures to a domain error (not a panic, §5.10).
    let content = serde_json::to_string(&message.content).map_err(|error| {
        roleplayer_core::errors::AppError::Domain(error.to_string())
    })?;
    // Placeholders ?1..?7 bind in column order; binding, never string-building,
    // keeps user data out of the SQL text (§5.16).
    storage
        .execute(
            "INSERT INTO messages (id, campaign_id, role, content, model, turn_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                &message.id,
                &message.campaign_id,
                // The role enum travels as its wire string, matching map_message.
                &message.role.as_str(),
                &content,
                // Option<String> binds as NULL when no model name is recorded.
                &message.model,
                &message.turn_index,
                &message.created_at,
            ],
        )
        // Row count is uninteresting for an insert; map to unit.
        .map(|_changed| ())
}

/// The highest turn index seen for a campaign; 0 when the transcript is empty.
pub fn latest_turn_index<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<i64> {
    // COALESCE keeps an empty transcript at turn 0 instead of NULL; the
    // outer unwrap_or is a second guard for the same case.
    storage
        .query_row(
            "SELECT COALESCE(MAX(turn_index), 0) FROM messages WHERE campaign_id = ?1",
            &[&campaign_id],
            |row| row.get(0),
        )
        // The aggregated row always exists (aggregate without GROUP BY returns
        // one row), so only the unwrap_or guard matters in practice.
        .map(|value| value.unwrap_or(0))
}

/// Recent messages of a campaign, oldest-first, capped at `limit`.
pub fn list_messages<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    limit: i64,
) -> Result<Vec<MessageDto>> {
    storage.query_vec(
        // Oldest-first transcript window; the campaign filter and LIMIT are
        // both bound so no other campaign's rows can leak in.
        "SELECT id, campaign_id, role, content, model, turn_index, created_at
         FROM messages
         WHERE campaign_id = ?1
         ORDER BY created_at ASC
         LIMIT ?2",
        &[&campaign_id, &limit],
        map_message,
    )
}

/// The most recent `window` messages for context building (newest kept).
pub fn recent_messages<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    window: i64,
) -> Result<Vec<MessageDto>> {
    storage.query_vec(
        // Fetches the NEWEST `window` rows (DESC + LIMIT) for context building.
        "SELECT id, campaign_id, role, content, model, turn_index, created_at
         FROM messages
         WHERE campaign_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2",
        &[&campaign_id, &window],
        map_message,
    )
    .map(|mut messages| {
        // ORDER BY created_at DESC fetches the newest `window`; reverse to
        // hand the caller oldest-first context ordering for prompt building.
        messages.reverse();
        messages
    })
}

/// Messages within a turn range, oldest-first (used by memory summarization).
pub fn list_messages_between<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    from_turn: i64,
    to_turn: i64,
) -> Result<Vec<MessageDto>> {
    storage.query_vec(
        // Inclusive range slice [from_turn, to_turn], oldest first — the exact
        // window memory summarization condenses into a Memory.
        "SELECT id, campaign_id, role, content, model, turn_index, created_at
         FROM messages
         WHERE campaign_id = ?1 AND turn_index >= ?2 AND turn_index <= ?3
         ORDER BY created_at ASC",
        &[&campaign_id, &from_turn, &to_turn],
        map_message,
    )
}

fn map_message(row: &Row<'_>) -> rusqlite::Result<MessageDto> {
    // Pull the JSON text out first so the tolerant parse is explicit.
    let content: String = row.get("content")?;
    Ok(MessageDto {
        id: row.get("id")?,
        campaign_id: row.get("campaign_id")?,
        // The role wire string is reversed by the shared Role parser; unknown
        // values degrade to a safe default inside core (§5.10).
        role: roleplayer_core::llm::Role::from_wire(
            &row.get::<_, String>("role")?,
        ),
        // Content is JSON text; a malformed block list degrades to an empty
        // transcript entry rather than failing the whole read (§5.10).
        content: serde_json::from_str(&content).unwrap_or_default(),
        model: row.get("model")?,
        turn_index: row.get("turn_index")?,
        created_at: row.get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::llm::{ContentBlock, Role};
    use roleplayer_core::storage::Database;
    use roleplayer_core::{new_id, now_rfc3339};

    fn seed_campaign(storage: &Database, campaign_id: &str) {
        // The messages FK requires a parent campaign row; seed one with the
        // minimal columns the schema accepts.
        storage
            .execute(
                "INSERT INTO campaigns (id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, '', '', '')",
                &[&campaign_id.to_string(), &campaign_id.to_string()],
            )
            .expect("seed campaign");
    }

    fn message(
        campaign_id: &str,
        role: Role,
        text: &str,
        turn: i64,
    ) -> MessageDto {
        MessageDto {
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            role,
            // A single text block; enough for transcript round-trip tests.
            content: vec![ContentBlock::Text { text: text.to_string() }],
            model: None,
            turn_index: turn,
            created_at: now_rfc3339(),
        }
    }

    #[test]
    fn transcript_round_trips_and_counts_turns() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");

        insert_message(&storage, &message("camp-1", Role::User, "hello", 1))
            .expect("insert");
        insert_message(&storage, &message("camp-1", Role::Assistant, "hi", 1))
            .expect("insert");
        insert_message(&storage, &message("camp-1", Role::User, "again", 2))
            .expect("insert");

        // The highest recorded turn index is 2 (from the second user turn).
        assert_eq!(latest_turn_index(&storage, "camp-1").expect("latest"), 2);

        // Full list is oldest-first and includes every inserted message.
        let messages = list_messages(&storage, "camp-1", 10).expect("list");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content[0].text(), Some("hello"));

        // The most recent 2, returned oldest-first for prompt building.
        let recent = recent_messages(&storage, "camp-1", 2).expect("recent");
        assert_eq!(recent.len(), 2);
        // Newest last (oldest-first ordering preserved).
        assert_eq!(recent[1].content[0].text(), Some("again"));
    }
}
