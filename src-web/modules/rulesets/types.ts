// Ruleset wire types — mirror the Rust `Ruleset` struct.

/** The GM's "brain" preset: a system prompt plus a world-rules JSON blob.
 *  `is_builtin` marks read-only rulesets shipped with the app (protected
 *  from edit/delete in the UI). */
export interface Ruleset {
  /** Row UUID, backend-generated. */
  id: string
  /** Display name shown in the manager's card titles. */
  name: string
  /** The system prompt that defines the GM's behavior (the "brain"). */
  system_prompt: string
  /** JSON blob of world/mechanic rules; untyped because rules are free-form
   *  and the app only stores what the user writes. */
  world_rules: unknown
  /** Read-only flag for rulesets shipped with the app; those can't be
   *  edited or deleted in the UI. */
  is_builtin: boolean
  /** When the ruleset was created (ISO timestamp, backend-assigned). */
  created_at: string
}

/** Create payload — a new ruleset is never built-in by default. */
export interface NewRuleset {
  name: string
  system_prompt: string
  world_rules: unknown
}

/** Full-shape update payload (the UI sends the whole ruleset back). */
export interface UpdateRuleset {
  name: string
  system_prompt: string
  world_rules: unknown
}
