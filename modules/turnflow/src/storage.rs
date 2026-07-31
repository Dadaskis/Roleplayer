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
    let content = serde_json::to_string(&message.content).map_err(|error| {
        roleplayer_core::errors::AppError::Domain(error.to_string())
    })?;
    storage
        .execute(
            "INSERT INTO messages (id, campaign_id, role, content, model, turn_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                &message.id,
                &message.campaign_id,
                &message.role.as_str(),
                &content,
                &message.model,
                &message.turn_index,
                &message.created_at,
            ],
        )
        .map(|_changed| ())
}

/// The highest turn index seen for a campaign; 0 when the transcript is empty.
pub fn latest_turn_index<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
) -> Result<i64> {
    storage
        .query_row(
            "SELECT COALESCE(MAX(turn_index), 0) FROM messages WHERE campaign_id = ?1",
            &[&campaign_id],
            |row| row.get(0),
        )
        .map(|value| value.unwrap_or(0))
}

/// Recent messages of a campaign, oldest-first, capped at `limit`.
pub fn list_messages<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    limit: i64,
) -> Result<Vec<MessageDto>> {
    storage.query_vec(
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
        "SELECT id, campaign_id, role, content, model, turn_index, created_at
         FROM messages
         WHERE campaign_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2",
        &[&campaign_id, &window],
        map_message,
    )
    .map(|mut messages| {
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
        "SELECT id, campaign_id, role, content, model, turn_index, created_at
         FROM messages
         WHERE campaign_id = ?1 AND turn_index >= ?2 AND turn_index <= ?3
         ORDER BY created_at ASC",
        &[&campaign_id, &from_turn, &to_turn],
        map_message,
    )
}

fn map_message(row: &Row<'_>) -> rusqlite::Result<MessageDto> {
    let content: String = row.get("content")?;
    Ok(MessageDto {
        id: row.get("id")?,
        campaign_id: row.get("campaign_id")?,
        role: roleplayer_core::llm::Role::from_wire(
            &row.get::<_, String>("role")?,
        ),
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

        assert_eq!(latest_turn_index(&storage, "camp-1").expect("latest"), 2);

        let messages = list_messages(&storage, "camp-1", 10).expect("list");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content[0].text(), Some("hello"));

        let recent = recent_messages(&storage, "camp-1", 2).expect("recent");
        assert_eq!(recent.len(), 2);
        // Newest last (oldest-first ordering preserved).
        assert_eq!(recent[1].content[0].text(), Some("again"));
    }
}
