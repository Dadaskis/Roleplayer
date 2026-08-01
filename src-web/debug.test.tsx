// Debug window render smoke test: verifies the pop-out root (world/characters/
// memories/search at full width) renders and its close button works — the one
// window-specific path the App suite can't cover (§5.11).

// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// The debug window uses the Tauri window API to close itself; stub it so the
// close button can be asserted without a real window. close() must return a
// promise because the component awaits/catches it. invoke/listen are stubbed
// like in the App suite (the panels' queries resolve from canned data).
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ close: vi.fn(() => Promise.resolve()) })),
}))
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

import { DebugRoot } from "./debug"
import { invoke } from "@tauri-apps/api/core"
import { getCurrentWindow } from "@tauri-apps/api/window"

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>
const closeMock = getCurrentWindow as unknown as ReturnType<typeof vi.fn>

// The campaign the debug root looks up for its title bar.
const campaign = {
  id: "c1",
  name: "The Duskmoor Pact",
  description: "",
  ruleset_id: null,
  status: "active",
  settings: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-02T00:00:00Z",
}

function renderDebug() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <DebugRoot campaignId="c1" />
    </QueryClientProvider>,
  )
}

describe("debug window", () => {
  beforeEach(() => {
    // Canned backend responses: every panel query resolves to empty data.
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "get_campaign":
          return Promise.resolve(campaign)
        case "get_world_state":
          return Promise.resolve({})
        case "list_state_changes":
          return Promise.resolve([])
        case "list_characters":
          return Promise.resolve([])
        case "list_memories":
          return Promise.resolve([])
        default:
          return Promise.resolve(null)
      }
    })
  })

  it("renders the section tabs and switches panels", async () => {
    renderDebug()
    // The title names the campaign; the World panel (default tab) renders.
    expect(await screen.findByText(/Debug — The Duskmoor Pact/)).toBeInTheDocument()
    expect(screen.getByText("Add fact")).toBeInTheDocument()

    // Switching tabs swaps the panel content at full width. The panel's
    // submit button is a unique role; the same string also appears as the
    // card's heading, so query by role to avoid the ambiguity.
    fireEvent.click(screen.getByRole("button", { name: "Characters" }))
    expect(await screen.findByRole("button", { name: "Add character" })).toBeInTheDocument()
  })

  it("closes the window via its × button", () => {
    renderDebug()
    // The close button calls the (mocked) window API's close.
    fireEvent.click(screen.getByLabelText("Close debug window"))
    expect(closeMock).toHaveBeenCalled()
    // getCurrentWindow() returned a stub whose close() is called once.
    const windowStub = closeMock.mock.results[0]?.value
    expect(windowStub.close).toHaveBeenCalledTimes(1)
  })
})
