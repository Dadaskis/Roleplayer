// Campaign wire types — mirror the Rust `Campaign`/`NewCampaign` structs.

/** A persisted campaign. `settings` is an opaque JSON bag the GM may fill;
 *  `ruleset_id` links the GM's prompt preset when one is chosen. */
export interface Campaign {
  // Backend-generated UUID; never trusted from the client.
  id: string
  name: string
  description: string
  // Null when no ruleset is bound; a full join is the backend's concern.
  ruleset_id: string | null
  // Arbitrary JSON the GM may grow; untyped here by design (§5.4 JSON-first).
  settings: unknown
  // ISO-8601 creation stamp, set once by the backend.
  created_at: string
  // ISO-8601 last-write stamp; the list screen uses it only for ordering.
  updated_at: string
}

/** Create payload — no id/timestamps; the backend generates and stamps them. */
export interface NewCampaign {
  name: string
  description: string
  ruleset_id: string | null
}

/** Full-shape update payload: the UI always sends the whole campaign back
 *  (no partial patches), mirroring how the Rust service validates updates. */
export interface UpdateCampaign {
  name: string
  description: string
  ruleset_id: string | null
}
