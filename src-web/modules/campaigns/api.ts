// Typed IPC calls for the campaigns module (contract mirror of Rust commands).

import { call } from "../../core/api/invoke"
import type { Campaign, NewCampaign, UpdateCampaign } from "./types"

export function listCampaigns(): Promise<Campaign[]> {
  return call<Campaign[]>("list_campaigns")
}

export function createCampaign(input: NewCampaign): Promise<Campaign> {
  return call<Campaign>("create_campaign", { input })
}

export function getCampaign(campaignId: string): Promise<Campaign | null> {
  return call<Campaign | null>("get_campaign", { campaignId })
}

export function updateCampaign(campaignId: string, input: UpdateCampaign): Promise<Campaign | null> {
  return call<Campaign | null>("update_campaign", { campaignId, input })
}

export function deleteCampaign(campaignId: string): Promise<boolean> {
  return call<boolean>("delete_campaign", { campaignId })
}
