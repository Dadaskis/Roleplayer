//! Pure world-state domain: the audit-trail entity. No I/O (§5.2).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One recorded world mutation — the audit trail of the GM's actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// Backend-generated UUID (v4); clients never supply it (§5.4).
    pub id: String,
    /// Owning campaign; audit rows are always scoped to one campaign.
    pub campaign_id: String,
    /// Which tool (or "manual" for direct UI edits) caused the change.
    pub tool: String,
    /// The raw arguments passed to the tool.
    pub args: Value,
    /// Snapshot of the key's value before the change (null = absent).
    pub before_value: Value,
    /// Snapshot of the key's value after the change.
    pub after_value: Value,
    /// The transcript message that triggered it, when known.
    pub message_id: Option<String>,
    /// RFC 3339 timestamp; audit rows are listed newest first.
    pub created_at: String,
}
