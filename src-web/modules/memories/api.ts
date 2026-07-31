// Typed IPC calls for the memories module.

import { call } from "../../core/api/invoke"
import type { Memory, NewMemory, SummarizeRequest } from "./types"

export function listMemories(campaignId: string): Promise<Memory[]> {
  return call<Memory[]>("list_memories", { campaignId })
}

export function createMemory(input: NewMemory): Promise<Memory> {
  return call<Memory>("create_memory", { input })
}

export function deleteMemory(memoryId: string): Promise<boolean> {
  return call<boolean>("delete_memory", { memoryId })
}

export function summarizeMemory(input: SummarizeRequest): Promise<Memory> {
  return call<Memory>("summarize_memory", { input })
}
