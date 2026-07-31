// Typed IPC calls for the rulesets module.

import { call } from "../../core/api/invoke"
import type { NewRuleset, Ruleset, UpdateRuleset } from "./types"

// One thin wrapper per Rust command; `input` payloads mirror the create/update
// wire types above, and full-shape updates keep the contract simple.
export function listRulesets(): Promise<Ruleset[]> {
  return call<Ruleset[]>("list_rulesets")
}

// Fetch one ruleset by id; `null` means the id doesn't exist (deleted).
export function getRuleset(rulesetId: string): Promise<Ruleset | null> {
  return call<Ruleset | null>("get_ruleset", { rulesetId })
}

// Persist a new ruleset; resolves with the stored row (id + timestamp added
// by the backend, `is_builtin` always false for user-made ones).
export function createRuleset(input: NewRuleset): Promise<Ruleset> {
  return call<Ruleset>("create_ruleset", { input })
}

// Full-shape update; the screen ships the whole object back, so this keeps
// the contract simple (no partial-patch semantics to drift).
export function updateRuleset(rulesetId: string, input: UpdateRuleset): Promise<Ruleset | null> {
  return call<Ruleset | null>("update_ruleset", { rulesetId, input })
}

// Remove a ruleset by id; `false` signals nothing was deleted (built-in or
// already gone). The screen hides Delete for built-ins anyway.
export function deleteRuleset(rulesetId: string): Promise<boolean> {
  return call<boolean>("delete_ruleset", { rulesetId })
}
