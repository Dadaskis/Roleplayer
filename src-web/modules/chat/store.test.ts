// Chat store logic tests (headless, §5.11): streaming accumulation and
// idempotent message handling.

import { beforeEach, describe, expect, it } from "vitest"

import { useChatStore } from "./store"
import type { MessageDto } from "../../core/api/invoke"

function userMessage(id: string, text: string): MessageDto {
  return {
    id,
    campaign_id: "c1",
    role: "user",
    content: [{ type: "text", text }],
    model: null,
    turn_index: 1,
    created_at: "2026-01-01T00:00:00Z",
  }
}

describe("chat store", () => {
  beforeEach(() => {
    useChatStore.setState({ byCampaign: {}, drafts: {}, streaming: {}, activity: {}, errors: {} })
  })

  it("accumulates streaming deltas into the draft", () => {
    const store = useChatStore.getState()
    store.appendDelta("c1", "The goblin ")
    store.appendDelta("c1", "snarls.")
    expect(useChatStore.getState().drafts["c1"]).toBe("The goblin snarls.")
  })

  it("appends messages without duplicates", () => {
    const store = useChatStore.getState()
    const message = userMessage("m1", "I step forward.")
    store.appendMessage("c1", message)
    store.appendMessage("c1", message)
    expect(useChatStore.getState().byCampaign["c1"]).toHaveLength(1)
  })

  it("resets the draft when a message lands", () => {
    const store = useChatStore.getState()
    store.appendDelta("c1", "streaming...")
    store.appendMessage("c1", userMessage("m1", "final"))
    expect(useChatStore.getState().drafts["c1"]).toBe("")
  })

  it("seeds initial transcript per campaign", () => {
    const store = useChatStore.getState()
    store.setInitial("c1", [userMessage("m1", "a"), userMessage("m2", "b")])
    expect(useChatStore.getState().byCampaign["c1"]).toHaveLength(2)
  })
})
