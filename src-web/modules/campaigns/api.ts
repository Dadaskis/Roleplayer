// Typed IPC calls for the campaigns module (contract mirror of Rust commands).

// `call` is the one bridge to the Rust backend; every function below is just
// a typed name for one command, so screens never touch invoke() themselves.
import { call } from "../../core/api/invoke"
import type { Campaign, NewCampaign, UpdateCampaign } from "./types"

// Each function is a thin wrapper over one Tauri command (§5.2): the args
// object mirrors the command's serde parameters and the payload `input`
// mirrors the NewCampaign/UpdateCampaign wire types above.
// No command argument ever comes from a client id — the backend generates ids.
export function listCampaigns(): Promise<Campaign[]> {
  // No args: the list is global, there is nothing to scope it by.
  return call<Campaign[]>("list_campaigns")
}

export function createCampaign(input: NewCampaign): Promise<Campaign> {
  // Payload rides as `input` to match the Rust command's parameter struct.
  return call<Campaign>("create_campaign", { input })
}

export function getCampaign(campaignId: string): Promise<Campaign | null> {
  // Null when the id is unknown; the caller decides how to surface a miss.
  return call<Campaign | null>("get_campaign", { campaignId })
}

export function updateCampaign(campaignId: string, input: UpdateCampaign): Promise<Campaign | null> {
  // Full-shape update: both the row id and the whole input travel together.
  return call<Campaign | null>("update_campaign", { campaignId, input })
}

// Returns whether a row was actually removed (false when the id was unknown)
// so the UI can decide whether a refetch is needed.
export function deleteCampaign(campaignId: string): Promise<boolean> {
  return call<boolean>("delete_campaign", { campaignId })
}
