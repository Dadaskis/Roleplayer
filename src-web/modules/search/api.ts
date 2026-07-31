// Typed IPC calls for the search module.

import { call } from "../../core/api/invoke"
import type { SearchResult } from "./types"

// The one search command: FTS5 over the campaign's transcript.
// `limit` bounds results so a broad query can't flood the panel; the backend
// ranks matches and returns at most `limit` rows.
export function searchMessages(campaignId: string, query: string, limit: number): Promise<SearchResult[]> {
  // The raw query string is sent as-is; ranking happens on the backend, so
  // the UI only maps the returned rows into result cards.
  return call<SearchResult[]>("search_messages", { campaignId, query, limit })
}
