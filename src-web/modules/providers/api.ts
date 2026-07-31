// Typed IPC calls for the providers module.

import { call } from "../../core/api/invoke"
import type { ModelInfo, ProviderInfo } from "./types"

// Fetch every configured provider; the screen maps these into picker cards.
export function listProviders(): Promise<ProviderInfo[]> {
  return call<ProviderInfo[]>("list_providers")
}

// Fetch a provider's model catalog; called only once a provider is selected
// (the screen gates this with the query's `enabled` flag).
export function listModels(providerId: string): Promise<ModelInfo[]> {
  return call<ModelInfo[]>("list_models", { providerId })
}

// Persist non-secret provider settings (which model, which endpoint). The key
// is written separately because it must go to the keyring, not the DB.
export function setProviderConfig(providerId: string, input: { model: string; base_url: string }): Promise<ProviderInfo> {
  return call<ProviderInfo>("set_provider_config", { providerId, input })
}

// Keys are written to the OS keyring by the backend and never returned in
// responses — the API key is a one-way write from the UI's perspective.
export function setProviderApiKey(providerId: string, apiKey: string): Promise<ProviderInfo> {
  return call<ProviderInfo>("set_provider_api_key", { providerId, apiKey })
}

// Wipe the stored key from the keyring; the UI disables this until `has_key`.
export function clearProviderApiKey(providerId: string): Promise<ProviderInfo> {
  return call<ProviderInfo>("clear_provider_api_key", { providerId })
}

// The default provider is the one turnflow uses when no campaign override is
// set; returns the newly-defaulted ProviderInfo so the UI can re-render badges.
export function setDefaultProvider(providerId: string): Promise<ProviderInfo> {
  return call<ProviderInfo>("set_default_provider", { providerId })
}

// Returns a human-readable result string (latency/round-trip), not a struct —
// the screen renders it directly as a status line.
export function testProvider(providerId: string): Promise<string> {
  return call<string>("test_provider", { providerId })
}
