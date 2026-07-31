//! Pure search domain: the result entity. No I/O (§5.2).

use roleplayer_core::llm::{ContentBlock, Role};
use serde::{Deserialize, Serialize};

/// A transcript row that matched the search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub message_id: String,
    pub campaign_id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub turn_index: i64,
    pub created_at: String,
    /// A short extract around the first match, when the FTS layer provides one.
    pub snippet: Option<String>,
}
