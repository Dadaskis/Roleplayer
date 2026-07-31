// Search wire types — mirror the Rust `SearchResult` struct.

import type { ContentBlock, Role } from "../../core/api/invoke"

export interface SearchResult {
  message_id: string
  campaign_id: string
  role: Role
  content: ContentBlock[]
  turn_index: number
  created_at: string
  snippet: string | null
}
