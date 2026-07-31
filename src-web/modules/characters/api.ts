// Typed IPC calls for the characters module.

// Typed IPC surface for characters; `call` stays the only backend bridge.
import { call } from "../../core/api/invoke"
import type { Character, NewCharacter, UpdateCharacter } from "./types"

// Every call mirrors one Rust command's serde signature — campaign scoping
// rides as an explicit id argument, payloads as an `input` object.
export function listCharacters(campaignId: string): Promise<Character[]> {
  // Scoped by campaign: the roster always belongs to the open workspace.
  return call<Character[]>("list_characters", { campaignId })
}

export function createCharacter(input: NewCharacter): Promise<Character> {
  // The input carries campaign_id; the backend returns the stamped row.
  return call<Character>("create_character", { input })
}

export function updateCharacter(characterId: string, input: UpdateCharacter): Promise<Character | null> {
  // Full-shape update; null when the id is unknown to the backend.
  return call<Character | null>("update_character", { characterId, input })
}

export function deleteCharacter(characterId: string): Promise<boolean> {
  // Boolean tells the caller whether a row actually disappeared.
  return call<boolean>("delete_character", { characterId })
}
