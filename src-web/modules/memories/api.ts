// Typed IPC calls for the memories module.

import { call } from "../../core/api/invoke"
import type { Memory, NewMemory, SummarizeRequest } from "./types"

// Fetch all long-term facts for a campaign; the panel renders them as cards.
export function listMemories(campaignId: string): Promise<Memory[]> {
  return call<Memory[]>("list_memories", { campaignId })
}

// Persist a hand-written memory; resolves with the stored row (id + timestamps
// assigned by the backend).
export function createMemory(input: NewMemory): Promise<Memory> {
  return call<Memory>("create_memory", { input })
}

// Removes a memory by id; the boolean confirms whether a row was actually
// deleted (false = already gone), which the panel ignores.
export function deleteMemory(memoryId: string): Promise<boolean> {
  return call<boolean>("delete_memory", { memoryId })
}

// Summarization is async provider work on the backend; resolves with the
// generated memory once it is persisted (see the spinner in the UI).
export function summarizeMemory(input: SummarizeRequest): Promise<Memory> {
  return call<Memory>("summarize_memory", { input })
}
