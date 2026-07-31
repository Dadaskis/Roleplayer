// Provider wire types — mirror the Rust `ProviderInfo`/`ModelInfo` structs.

// The adapter kinds the backend can instantiate; `mock` is the reference
// implementation every provider must match (§5.5).
export type ProviderKind = "mock" | "openai_compatible"

/** A configured provider. `has_key` is a flag only — the key itself lives in
 *  the OS keyring, never in the DB or on the wire (§5.4). */
export interface ProviderInfo {
  /** Provider row UUID, backend-generated. */
  id: string
  /** Human-readable provider name shown in the picker. */
  name: string
  /** Which adapter the backend instantiates for this provider. */
  kind: ProviderKind
  /** Endpoint the adapter talks to (empty/"mock" for the Mock provider). */
  base_url: string
  /** Currently selected model id — seeds the picker when the card opens. */
  model: string
  /** Whether a key is stored in the OS keyring; the key itself never leaves
   *  the backend (§5.4). */
  has_key: boolean
  /** Whether turnflow uses this provider when no campaign override is set. */
  is_default: boolean
}

/** A model in a provider's catalog; `null` capability values mean the catalog
 *  didn't say, so the app should not assume a number. */
export interface ModelInfo {
  /** Catalog model id sent to the provider on requests. */
  id: string
  /** Display name for the model dropdown. */
  name: string
  /** Prompt+completion token budget, or null if the catalog was silent. */
  context_window: number | null
  /** Max tokens the model can emit per call, or null if unknown. */
  max_output: number | null
  /** Whether the model accepts tool calls — gates the GM's tool use. */
  supports_tools: boolean
}
