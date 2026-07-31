// Memory wire types — mirror the Rust `Memory` struct.

/** A long-term fact covering transcript turns [source_from, source_to]; it
 *  survives context-window trimming so key plot points are not forgotten. */
export interface Memory {
  /** Row UUID, backend-generated (§5.4). */
  id: string
  /** Campaign the memory belongs to; scopes the list query. */
  campaign_id: string
  /** Condensed fact text, either hand-written by the GM or generated. */
  summary: string
  /** First transcript turn the memory distills (inclusive). */
  source_from: number
  /** Last transcript turn the memory distills (inclusive). */
  source_to: number
  /** When the memory was persisted (ISO timestamp, added by backend). */
  created_at: string
}

/** Create payload for a hand-written memory (no timestamps — backend adds). */
export interface NewMemory {
  /** Target campaign; the backend owns the UUID generation. */
  campaign_id: string
  /** The fact to remember verbatim. */
  summary: string
  /** Anchors a hand-written memory to no turns — the 0..0 sentinel. */
  source_from: number
  source_to: number
}

/** Ask the provider to condense a turn range into a memory; an open-ended
 *  range (e.g. 0..0, "as far back as available") is resolved by turnflow. */
export interface SummarizeRequest {
  /** Campaign whose transcript will be summarized. */
  campaign_id: string
  /** Start of the range to condense (0 = "as far back as available"). */
  source_from: number
  /** End of the range to condense (0 = "as far as available"). */
  source_to: number
}
