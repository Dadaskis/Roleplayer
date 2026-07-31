// World-state wire types — the audit-trail entity (§4.6 of PLAN.md).

/** One recorded world mutation — the anti-hallucination audit trail (§4.6).
 *  It pairs the tool's args with before/after values so every change can be
 *  traced and, in principle, reverted; `message_id` links it to the GM turn. */
export interface StateChange {
  /** Row UUID, backend-generated (§5.4: never client-provided). */
  id: string
  /** Campaign whose world document was mutated; scopes the audit query. */
  campaign_id: string
  /** Name of the game command / tool that performed the mutation. */
  tool: string
  /** Raw arguments the tool was invoked with — untyped because each tool
   *  (dice, world update, ...) has its own arg shape. */
  args: unknown
  /** Snapshot of the affected world key before the write, so the change
   *  can be diffed or reverted in principle. */
  before_value: unknown
  /** Snapshot of the affected world key after the write. */
  after_value: unknown
  /** GM turn that caused the mutation; null when the edit was manual. */
  message_id: string | null
  /** When the mutation was persisted (ISO timestamp, added by backend). */
  created_at: string
}
