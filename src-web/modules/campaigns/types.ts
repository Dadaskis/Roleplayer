// Campaign wire types — mirror the Rust `Campaign`/`NewCampaign` structs.

export interface Campaign {
  id: string
  name: string
  description: string
  ruleset_id: string | null
  settings: unknown
  created_at: string
  updated_at: string
}

export interface NewCampaign {
  name: string
  description: string
  ruleset_id: string | null
}

export interface UpdateCampaign {
  name: string
  description: string
  ruleset_id: string | null
}
