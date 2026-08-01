// Chat store logic tests (headless, §5.11): streaming accumulation and
// idempotent message handling.

// Store logic tests run headless (no Tauri, no window) against the real
// Zustand store — pure function-level coverage of streaming accumulation
// and idempotent message handling (§5.11, §7).

import { beforeEach, describe, expect, it } from "vitest"

import { useChatStore } from "./store"
import type { MessageDto } from "../../core/api/invoke"

// Build a minimal valid user message for a fixed campaign so tests stay
// focused on store behavior rather than DTO plumbing.
function userMessage(id: string, text: string): MessageDto {
  return {
    id,
    campaign_id: "c1",
    role: "user",
    content: [{ type: "text", text }],
    // Default mode; the store treats all modes identically.
    mode: "action",
    model: null,
    turn_index: 1,
    // Any fixed timestamp is fine; the store never reads it.
    created_at: "2026-01-01T00:00:00Z",
  }
}

describe("chat store", () => {
  beforeEach(() => {
    // Reset the store to pristine state between tests — Zustand stores are
    // module singletons and would otherwise leak state across cases.
    // setState with a partial object replaces just these five keys, matching
    // the initialState template the store was created with.
    useChatStore.setState({ byCampaign: {}, drafts: {}, streaming: {}, activity: {}, errors: {} })
  })

  it("accumulates streaming deltas into the draft", () => {
    const store = useChatStore.getState()
    // First delta seeds the buffer for campaign c1.
    store.appendDelta("c1", "The goblin ")
    // Second delta appends to the existing buffer, not replaces it.
    store.appendDelta("c1", "snarls.")
    // Both deltas must concatenate in order into the accumulated draft.
    expect(useChatStore.getState().drafts["c1"]).toBe("The goblin snarls.")
  })

  it("appends messages without duplicates", () => {
    const store = useChatStore.getState()
    const message = userMessage("m1", "I step forward.")
    // First append: the message is new, so it lands in the transcript.
    store.appendMessage("c1", message)
    // Second append with the same id: the dedup guard must reject it.
    store.appendMessage("c1", message)
    // Exactly one row — a duplicate event must never double-render a bubble.
    expect(useChatStore.getState().byCampaign["c1"]).toHaveLength(1)
  })

  it("resets the draft when a message lands", () => {
    const store = useChatStore.getState()
    // Simulate mid-stream: the buffer holds provisional text.
    store.appendDelta("c1", "streaming...")
    // The authoritative message arrives and takes the draft's place.
    store.appendMessage("c1", userMessage("m1", "final"))
    // The draft must be cleared so the live bubble doesn't linger.
    expect(useChatStore.getState().drafts["c1"]).toBe("")
  })

  it("seeds initial transcript per campaign", () => {
    const store = useChatStore.getState()
    // Seed behaves like a fresh mount: it overwrites the transcript wholesale.
    store.setInitial("c1", [userMessage("m1", "a"), userMessage("m2", "b")])
    // Both messages from the seed must be present after the set.
    expect(useChatStore.getState().byCampaign["c1"]).toHaveLength(2)
  })
})
