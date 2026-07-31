// Chat state: transcripts + the live streaming buffer, keyed by campaign.
//
// Streaming model: the backend emits `turn_delta` events as the GM's text
// streams and a final `turn_message` when it settles. The store keeps a per-
// campaign `draft` (accumulated text) rendered as a live bubble, which is
// replaced by the authoritative message on `turn_message` / `turn_complete`.

import { create } from "zustand"
import type { MessageDto } from "../../core/api/invoke"

interface ChatState {
  /** Persisted transcript per campaign (source of truth once loaded). */
  byCampaign: Record<string, MessageDto[]>
  /** In-flight GM text, accumulated from turn_delta events. */
  drafts: Record<string, string>
  /** Whether a turn is actively streaming, per campaign. */
  streaming: Record<string, boolean>
  /** Transient status line during a turn (e.g. "GM used tool: …"), not part
   *  of the transcript; cleared when the turn settles. */
  activity: Record<string, string>
  /** Last user-visible error for a campaign, if any. */
  errors: Record<string, string | null>
  // Replace a campaign's whole transcript (first load / refetch seed).
  setInitial: (campaignId: string, messages: MessageDto[]) => void
  // Append one persisted message to a campaign's transcript (dedup'd).
  appendMessage: (campaignId: string, message: MessageDto) => void
  // Accumulate streamed prose into the live draft buffer.
  appendDelta: (campaignId: string, delta: string) => void
  // Set/clear the transient status line for a campaign.
  setActivity: (campaignId: string, text: string | null) => void
  // Flip the streaming lock (composer disabled while true).
  setStreaming: (campaignId: string, on: boolean) => void
  // Wipe the draft buffer for a campaign.
  resetDraft: (campaignId: string) => void
  // Set/clear the last user-visible error for a campaign.
  setError: (campaignId: string, message: string | null) => void
}

// Pristine starting state, reused as the template for test resets too.
const initialState = {
  byCampaign: {},
  drafts: {},
  streaming: {},
  activity: {},
  errors: {},
}

export const useChatStore = create<ChatState>((set) => ({
  // Start from the empty state above (spread keeps these five keys aligned).
  ...initialState,

  // Seed the transcript on load and clear any leftover draft/error: a fresh
  // mount (campaign switch or refetch) must never show stale streaming state.
  setInitial: (campaignId, messages) =>
    set((state) => ({
      // Replace in place: any previously loaded transcript for this campaign
      // is superseded by the authoritative array from the backend.
      byCampaign: { ...state.byCampaign, [campaignId]: messages },
      drafts: { ...state.drafts, [campaignId]: "" },
      errors: { ...state.errors, [campaignId]: null },
    })),

  appendMessage: (campaignId, message) =>
    set((state) => {
      // Default to an empty list when the campaign has no transcript yet.
      const current = state.byCampaign[campaignId] ?? []
      // Avoid duplicating a message we already hold (idempotent events).
      if (current.some((existing) => existing.id === message.id)) {
        return state
      }
      return {
        byCampaign: { ...state.byCampaign, [campaignId]: [...current, message] },
        // Clearing the draft here is the handoff: the authoritative message
        // replaced the live bubble, so buffered delta text is now redundant.
        drafts: { ...state.drafts, [campaignId]: "" },
      }
    }),

  // Plain string append: the draft is a single GM turn, so O(n) per delta is
  // fine and the buffer stays trivial to reason about (vs. a rope/queue).
  appendDelta: (campaignId, delta) =>
    set((state) => ({
      // String concat is the whole streaming model — each delta extends the
      // existing buffer, starting from "" if this campaign never streamed.
      drafts: { ...state.drafts, [campaignId]: (state.drafts[campaignId] ?? "") + delta },
    })),

  // `?? ""` keeps record values uniform strings (subscribers check truthiness,
  // so "" reads as cleared) — avoids nullable plumbing through the UI.
  setActivity: (campaignId, text) =>
    set((state) => ({ activity: { ...state.activity, [campaignId]: text ?? "" } })),

  // Streaming gates the composer (disable send) and toggles the live bubble;
  // it is per-campaign so switching campaigns never bleeds state across.
  setStreaming: (campaignId, on) =>
    set((state) => ({ streaming: { ...state.streaming, [campaignId]: on } })),

  // Draft is wiped by both paths that end a turn: the final message event
  // (appendMessage) and turn_complete — belt and braces against a leftover
  // half-rendered bubble.
  resetDraft: (campaignId) =>
    set((state) => ({ drafts: { ...state.drafts, [campaignId]: "" } })),

  // Straight overwrite of the per-campaign error; null clears it.
  setError: (campaignId, message) =>
    set((state) => ({ errors: { ...state.errors, [campaignId]: message } })),
}))
