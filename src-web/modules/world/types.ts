// World-state wire types — the audit-trail entity (§4.6 of PLAN.md).

export interface StateChange {
  id: string
  campaign_id: string
  tool: string
  args: unknown
  before_value: unknown
  after_value: unknown
  message_id: string | null
  created_at: string
}
