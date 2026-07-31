// Ruleset wire types — mirror the Rust `Ruleset` struct.

export interface Ruleset {
  id: string
  name: string
  system_prompt: string
  world_rules: unknown
  is_builtin: boolean
  created_at: string
}

export interface NewRuleset {
  name: string
  system_prompt: string
  world_rules: unknown
}

export interface UpdateRuleset {
  name: string
  system_prompt: string
  world_rules: unknown
}
