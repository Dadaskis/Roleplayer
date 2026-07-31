// Typed IPC calls for the characters module.

import { call } from "../../core/api/invoke"
import type { Character, NewCharacter, UpdateCharacter } from "./types"

export function listCharacters(campaignId: string): Promise<Character[]> {
  return call<Character[]>("list_characters", { campaignId })
}

export function createCharacter(input: NewCharacter): Promise<Character> {
  return call<Character>("create_character", { input })
}

export function updateCharacter(characterId: string, input: UpdateCharacter): Promise<Character | null> {
  return call<Character | null>("update_character", { characterId, input })
}

export function deleteCharacter(characterId: string): Promise<boolean> {
  return call<boolean>("delete_character", { characterId })
}
