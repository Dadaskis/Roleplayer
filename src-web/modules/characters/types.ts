// Character wire types — mirror the Rust `Character` struct.

/** A character in a campaign. `is_player` separates the user's persona from
 *  NPCs; `stats` and `extra` are free-form JSON the ruleset may interpret. */
export interface Character {
  // Backend-generated UUID identifying this character.
  id: string
  // Scoping key: characters belong to exactly one campaign.
  campaign_id: string
  name: string
  // True for the user's persona, false for NPCs; drives the badge tint.
  is_player: boolean
  // Free-text description shown under the name in the roster.
  bio: string
  // Rule-agnostic stat block; the ruleset owns the interpretation.
  stats: unknown
  // Reserved for future rule-specific extras (kept on the wire type now so
  // adding it later is not a breaking change).
  extra: unknown
  // ISO-8601 creation stamp from the backend.
  created_at: string
}

/** Create payload; the backend generates the id and created_at stamp. */
export interface NewCharacter {
  campaign_id: string
  name: string
  is_player: boolean
  bio: string
  stats: unknown
}

/** Full-shape update payload (no partial patches). */
export interface UpdateCharacter {
  name: string
  is_player: boolean
  bio: string
  stats: unknown
}
