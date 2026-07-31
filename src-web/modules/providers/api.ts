// Typed IPC calls for the providers module.

import { call } from "../../core/api/invoke"
import type { ModelInfo, ProviderInfo } from "./types"

export function listProviders(): Promise<ProviderInfo[]> {
  return call<ProviderInfo[]>("list_providers")
}

export function listModels(providerId: string): Promise<ModelInfo[]> {
  return call<ModelInfo[]>("list_models", { providerId })
}

export function setProviderConfig(providerId: string, input: { model: string; base_url: string }): Promise<ProviderInfo> {
  return call<ProviderInfo>("set_provider_config", { providerId, input })
}

export function setProviderApiKey(providerId: string, apiKey: string): Promise<ProviderInfo> {
  return call<ProviderInfo>("set_provider_api_key", { providerId, apiKey })
}

export function clearProviderApiKey(providerId: string): Promise<ProviderInfo> {
  return call<ProviderInfo>("clear_provider_api_key", { providerId })
}

export function setDefaultProvider(providerId: string): Promise<ProviderInfo> {
  return call<ProviderInfo>("set_default_provider", { providerId })
}

export function testProvider(providerId: string): Promise<string> {
  return call<string>("test_provider", { providerId })
}
