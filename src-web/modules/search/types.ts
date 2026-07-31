// Search wire types — mirror the Rust `SearchResult` struct.

import type { ContentBlock, Role } from "../../core/api/invoke"

/** A full-text match over a campaign's transcript. `snippet` is the FTS5
 *  highlighted extract when available; `content` mirrors MessageDto's blocks
 *  so a hit can render exactly like a transcript message. */
export interface SearchResult {
  /** Transcript message that matched; doubles as the card's React key. */
  message_id: string
  /** Campaign the match belongs to; scopes every search query. */
  campaign_id: string
  /** Who wrote the matched line (assistant/user), for the role badge. */
  role: Role
  /** The message's content blocks, so a hit renders like a transcript row. */
  content: ContentBlock[]
  /** Turn number of the match, shown as "turn N" in the card header. */
  turn_index: number
  /** When the matched message was created (ISO timestamp). */
  created_at: string
  /** FTS5-highlighted extract when the backend produced one, else null. */
  snippet: string | null
}
