// Character wire types — mirror the Rust `Character` struct.

export interface Character {
  id: string
  campaign_id: string
  name: string
  is_player: boolean
  bio: string
  stats: unknown
  extra: unknown
  created_at: string
}

export interface NewCharacter {
  campaign_id: string
  name: string
  is_player: boolean
  bio: string
  stats: unknown
}

export interface UpdateCharacter {
  name: string
  is_player: boolean
  bio: string
  stats: unknown
}
