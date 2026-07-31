// Provider wire types — mirror the Rust `ProviderInfo`/`ModelInfo` structs.

export type ProviderKind = "mock" | "openai_compatible"

export interface ProviderInfo {
  id: string
  name: string
  kind: ProviderKind
  base_url: string
  model: string
  has_key: boolean
  is_default: boolean
}

export interface ModelInfo {
  id: string
  name: string
  context_window: number | null
  max_output: number | null
  supports_tools: boolean
}
