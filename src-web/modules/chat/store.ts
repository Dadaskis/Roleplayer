// Chat state: transcripts + the live streaming buffer, keyed by campaign.
//
// Streaming model: the backend emits `turn_delta` events as the GM's text
// streams and a final `turn_message` when it settles. The store keeps a per-
// campaign `draft` (accumulated text) rendered as a live bubble, which is
// replaced by the authoritative message on `turn_message` / `turn_complete`.

import { create } from "zustand"
import type { MessageDto } from "../../core/api/invoke"

interface ChatState {
  byCampaign: Record<string, MessageDto[]>
  drafts: Record<string, string>
  streaming: Record<string, boolean>
  activity: Record<string, string>
  errors: Record<string, string | null>
  setInitial: (campaignId: string, messages: MessageDto[]) => void
  appendMessage: (campaignId: string, message: MessageDto) => void
  appendDelta: (campaignId: string, delta: string) => void
  setActivity: (campaignId: string, text: string | null) => void
  setStreaming: (campaignId: string, on: boolean) => void
  resetDraft: (campaignId: string) => void
  setError: (campaignId: string, message: string | null) => void
}

const initialState = {
  byCampaign: {},
  drafts: {},
  streaming: {},
  activity: {},
  errors: {},
}

export const useChatStore = create<ChatState>((set) => ({
  ...initialState,

  setInitial: (campaignId, messages) =>
    set((state) => ({
      byCampaign: { ...state.byCampaign, [campaignId]: messages },
      drafts: { ...state.drafts, [campaignId]: "" },
      errors: { ...state.errors, [campaignId]: null },
    })),

  appendMessage: (campaignId, message) =>
    set((state) => {
      const current = state.byCampaign[campaignId] ?? []
      // Avoid duplicating a message we already hold (idempotent events).
      if (current.some((existing) => existing.id === message.id)) {
        return state
      }
      return {
        byCampaign: { ...state.byCampaign, [campaignId]: [...current, message] },
        drafts: { ...state.drafts, [campaignId]: "" },
      }
    }),

  appendDelta: (campaignId, delta) =>
    set((state) => ({
      drafts: { ...state.drafts, [campaignId]: (state.drafts[campaignId] ?? "") + delta },
    })),

  setActivity: (campaignId, text) =>
    set((state) => ({ activity: { ...state.activity, [campaignId]: text ?? "" } })),

  setStreaming: (campaignId, on) =>
    set((state) => ({ streaming: { ...state.streaming, [campaignId]: on } })),

  resetDraft: (campaignId) =>
    set((state) => ({ drafts: { ...state.drafts, [campaignId]: "" } })),

  setError: (campaignId, message) =>
    set((state) => ({ errors: { ...state.errors, [campaignId]: message } })),
}))
