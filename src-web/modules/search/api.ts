// Typed IPC calls for the search module.

import { call } from "../../core/api/invoke"
import type { SearchResult } from "./types"

export function searchMessages(campaignId: string, query: string, limit: number): Promise<SearchResult[]> {
  return call<SearchResult[]>("search_messages", { campaignId, query, limit })
}
