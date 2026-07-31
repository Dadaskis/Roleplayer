// Typed IPC calls for the rulesets module.

import { call } from "../../core/api/invoke"
import type { NewRuleset, Ruleset, UpdateRuleset } from "./types"

export function listRulesets(): Promise<Ruleset[]> {
  return call<Ruleset[]>("list_rulesets")
}

export function getRuleset(rulesetId: string): Promise<Ruleset | null> {
  return call<Ruleset | null>("get_ruleset", { rulesetId })
}

export function createRuleset(input: NewRuleset): Promise<Ruleset> {
  return call<Ruleset>("create_ruleset", { input })
}

export function updateRuleset(rulesetId: string, input: UpdateRuleset): Promise<Ruleset | null> {
  return call<Ruleset | null>("update_ruleset", { rulesetId, input })
}

export function deleteRuleset(rulesetId: string): Promise<boolean> {
  return call<boolean>("delete_ruleset", { rulesetId })
}
