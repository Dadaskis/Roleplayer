// Typed IPC calls for the world-state module.

import { call } from "../../core/api/invoke"
import type { StateChange } from "./types"

export function getWorldState(campaignId: string): Promise<unknown> {
  return call<unknown>("get_world_state", { campaignId })
}

export function setWorldKey(campaignId: string, key: string, value: unknown): Promise<[unknown, unknown]> {
  return call<[unknown, unknown]>("set_world_key", { campaignId, key, value })
}

export function removeWorldKey(campaignId: string, key: string): Promise<[unknown, unknown]> {
  return call<[unknown, unknown]>("remove_world_key", { campaignId, key })
}

export function listStateChanges(campaignId: string, limit: number): Promise<StateChange[]> {
  return call<StateChange[]>("list_state_changes", { campaignId, limit })
}
