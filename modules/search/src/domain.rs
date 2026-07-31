//! Pure search domain: the result entity. No I/O (§5.2).

use roleplayer_core::llm::{ContentBlock, Role};
use serde::{Deserialize, Serialize};

/// A transcript row that matched the search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The matching message's id, so the UI can jump to it in the transcript.
    pub message_id: String,
    /// Owning campaign; search is always scoped to one campaign.
    pub campaign_id: String,
    /// Who said it (user/assistant); lets the UI style the result.
    pub role: Role,
    /// The message content blocks, so results render exactly like messages.
    pub content: Vec<ContentBlock>,
    /// Which turn produced the match; shown as context.
    pub turn_index: i64,
    /// RFC 3339 timestamp; results sort newest first by rank first.
    pub created_at: String,
    /// A short extract around the first match, when the FTS layer provides one.
    pub snippet: Option<String>,
}
