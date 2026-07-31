//! SQLite repository for FTS5 transcript search.

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;
use rusqlite::Row;

use crate::domain::SearchResult;

/// Search a campaign's transcript for `query`, newest-first, capped at `limit`.
///
/// The MATCH expression is built with quoted terms and an explicit campaign
/// filter so a hostile query string cannot inject FTS syntax (§5.10): every
/// term is wrapped in double quotes and embedded quotes are stripped.
pub fn search<S: Storage + ?Sized>(
    storage: &S,
    campaign_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<SearchResult>> {
    let match_expression = build_match_expression(campaign_id, query);
    if match_expression.is_empty() {
        // No usable terms survived sanitizing; return an empty result rather
        // than issuing a MATCH that would throw or match everything.
        return Ok(Vec::new());
    }

    // messages_fts is an external-content FTS5 table: it indexes messages but
    // stores no text, so we join back on rowid to fetch the real row. Column
    // index 2 of snippet() is `content`; `rank` (FTS relevance) sorts best.
    storage.query_vec(
        "SELECT m.id, m.campaign_id, m.role, m.content, m.turn_index, m.created_at,
                snippet(messages_fts, 2, '[', ']', '...', 12) AS snippet
         FROM messages_fts
         JOIN messages m ON m.rowid = messages_fts.rowid
         WHERE messages_fts MATCH ?1
         ORDER BY rank LIMIT ?2",
        // The MATCH expression is bound as a parameter even though it is query
        // text, so it cannot be interpreted as additional SQL (§5.16).
        &[&match_expression, &limit],
        map_result,
    )
}

/// Build a safe FTS5 MATCH expression scoped to a campaign.
fn build_match_expression(campaign_id: &str, query: &str) -> String {
    // Campaign scoping lives inside the MATCH expression itself (not a WHERE
    // clause) so the FTS index prunes rows before ranking — cheaper and it
    // keeps one safe filter for both scope and terms.
    let mut terms = vec![format!("campaign_id:\"{campaign_id}\"")];
    // Split on whitespace so "tavern ring" searches for two separate terms;
    // punctuation stays glued to its word, which FTS tokenizes away anyway.
    for raw_term in query.split_whitespace() {
        // Strip embedded quotes so a quote cannot break out of the phrase.
        let clean_term = raw_term.replace('"', "");
        if !clean_term.is_empty() {
            // Each term is scoped to the content column and phrase-quoted so
            // FTS treats it as a plain token, not a syntax fragment.
            terms.push(format!("content:\"{clean_term}\""));
        }
    }
    if terms.len() == 1 {
        // No usable query terms after sanitizing.
        return String::new();
    }
    // AND-join makes every term mandatory; FTS5 would AND them implicitly
    // anyway, but spelling it out keeps the multi-term intent explicit.
    terms.join(" AND ")
}

fn map_result(row: &Row<'_>) -> rusqlite::Result<SearchResult> {
    // The join pulls the raw content text from the real messages table.
    let content: String = row.get("content")?;
    Ok(SearchResult {
        message_id: row.get("id")?,
        campaign_id: row.get("campaign_id")?,
        // The role wire string is reversed by the shared Role parser.
        role: roleplayer_core::llm::Role::from_wire(
            &row.get::<_, String>("role")?,
        ),
        // Same JSON-with-fallback as the transcript repo (§5.10).
        content: serde_json::from_str(&content).unwrap_or_default(),
        turn_index: row.get("turn_index")?,
        created_at: row.get("created_at")?,
        // snippet() returns NULL when FTS has no highlighted extract; the
        // UI treats None as "no preview".
        snippet: row.get("snippet")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use roleplayer_core::llm::Role;
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

    fn insert_message(
        storage: &Database,
        campaign_id: &str,
        role: Role,
        text: &str,
    ) {
        // A single text block matches how the transcript repo serializes;
        // the FTS triggers pick up this row automatically.
        let content =
            serde_json::json!([{ "type": "text", "text": text }]).to_string();
        storage
            .execute(
                "INSERT INTO messages (id, campaign_id, role, content, model, turn_index, created_at)
                 VALUES (?1, ?2, ?3, ?4, NULL, 1, ?5)",
                &[
                    &new_id(),
                    &campaign_id.to_string(),
                    &role.as_str(),
                    &content,
                    &now_rfc3339(),
                ],
            )
            .expect("insert message");
    }

    #[test]
    fn finds_matching_transcript_rows() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");
        insert_message(
            &storage,
            "camp-1",
            Role::User,
            "I search the tavern for the missing ring",
        );
        insert_message(
            &storage,
            "camp-1",
            Role::Assistant,
            "You find a glinting ring under the bar.",
        );

        // "ring" appears in both messages, so both must match.
        let results = search(&storage, "camp-1", "ring", 10).expect("search");
        assert_eq!(results.len(), 2, "both messages mention the ring");

        // "glinting" is unique to the assistant's reply.
        let only_ring =
            search(&storage, "camp-1", "glinting", 10).expect("search");
        assert_eq!(only_ring.len(), 1);
        assert_eq!(only_ring[0].role, Role::Assistant);
    }

    #[test]
    fn scopes_results_to_campaign() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");
        seed_campaign(&storage, "camp-2");
        insert_message(&storage, "camp-1", Role::User, "ring here");
        insert_message(&storage, "camp-2", Role::User, "ring elsewhere");

        // The campaign filter inside the MATCH must hide camp-2's row.
        let results = search(&storage, "camp-1", "ring", 10).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].campaign_id, "camp-1");
    }

    #[test]
    fn empty_or_injected_queries_are_safe() {
        let storage = Database::open_in_memory().expect("in-memory db");
        seed_campaign(&storage, "camp-1");
        insert_message(&storage, "camp-1", Role::User, "plain text");

        // An empty query produces no terms, so the search short-circuits.
        assert!(search(&storage, "camp-1", "", 10)
            .expect("empty query")
            .is_empty());
        // A quote cannot break out of the FTS expression.
        let injected = search(&storage, "camp-1", "plain\" OR \"1", 10)
            .expect("injected query");
        assert!(
            injected.is_empty(),
            "injected query must not widen the search"
        );
    }
}
