// Campaign UI state: the active campaign + a refresh tick so screens can
// re-fetch after mutations without a manual cache invalidation dance.

import { create } from "zustand"

interface CampaignState {
  activeCampaignId: string | null
  campaignsVersion: number
  setActiveCampaign: (campaignId: string | null) => void
  bumpCampaigns: () => void
}

export const useCampaignStore = create<CampaignState>((set) => ({
  activeCampaignId: null,
  campaignsVersion: 0,
  setActiveCampaign: (campaignId) => set({ activeCampaignId: campaignId }),
  bumpCampaigns: () => set((state) => ({ campaignsVersion: state.campaignsVersion + 1 })),
}))
