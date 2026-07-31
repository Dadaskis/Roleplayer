// Typed IPC calls for the world-state module.

import { call } from "../../core/api/invoke"
import type { StateChange } from "./types"

// Reads the whole world document. The payload is `unknown` because the
// document is a free-form JSON blob owned by the GM — the shape is only
// validated at render time (see `WorldPanel`).
export function getWorldState(campaignId: string): Promise<unknown> {
  return call<unknown>("get_world_state", { campaignId })
}

// Mutations return [before, after] so the UI could surface the diff; the same
// pair is persisted to the audit trail by the backend on every write.
export function setWorldKey(campaignId: string, key: string, value: unknown): Promise<[unknown, unknown]> {
  return call<[unknown, unknown]>("set_world_key", { campaignId, key, value })
}

// Deleting a key is a destructive write, so it goes through the same audit
// path as `set_world_key` — the before/after pair records what was removed.
export function removeWorldKey(campaignId: string, key: string): Promise<[unknown, unknown]> {
  return call<[unknown, unknown]>("remove_world_key", { campaignId, key })
}

// Pulls the most recent audit rows; the caller decides how many to show
// (the panel asks for 30) so the trail cannot grow unbounded in the UI.
export function listStateChanges(campaignId: string, limit: number): Promise<StateChange[]> {
  return call<StateChange[]>("list_state_changes", { campaignId, limit })
}
