// Campaign UI state: the active campaign + a refresh tick so screens can
// re-fetch after mutations without a manual cache invalidation dance.

import { create } from "zustand"

interface CampaignState {
  /** Which campaign the workspace is showing (null = back on the list). */
  activeCampaignId: string | null
  /** Monotonic tick bumped on list mutations; cache layers can key off it. */
  campaignsVersion: number
  // Swaps the workspace's open campaign (null leaves the workspace).
  setActiveCampaign: (campaignId: string | null) => void
  // Signals "the list changed" so refetch triggers don't need manual wiring.
  bumpCampaigns: () => void
}

export const useCampaignStore = create<CampaignState>((set) => ({
  // No campaign selected at boot — the app opens on the list screen.
  activeCampaignId: null,
  // Version starts at zero; the first mutation bumps it to 1 and so on.
  campaignsVersion: 0,
  // UI-only state — "which campaign is open" is not persisted, so a relaunch
  // starts back on the list rather than re-entering a stale workspace.
  // Straight overwrite: opening campaign B simply replaces the id for A.
  setActiveCampaign: (campaignId) => set({ activeCampaignId: campaignId }),
  // Bump uses a reducer read of the current value, so two rapid bumps can
  // never both write the same number (each reads the previous increment).
  bumpCampaigns: () => set((state) => ({ campaignsVersion: state.campaignsVersion + 1 })),
}))
