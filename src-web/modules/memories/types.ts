// Memory wire types — mirror the Rust `Memory` struct.

export interface Memory {
  id: string
  campaign_id: string
  summary: string
  source_from: number
  source_to: number
  created_at: string
}

export interface NewMemory {
  campaign_id: string
  summary: string
  source_from: number
  source_to: number
}

export interface SummarizeRequest {
  campaign_id: string
  source_from: number
  source_to: number
}
