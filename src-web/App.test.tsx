// App shell render smoke test: verifies the stage-based flow works end to end
// in jsdom — lobby → chat → debug drawer → back — without a window (§5.11).
//
// The whole UI renders here, so this catches layout/JSX/runtime errors that
// store tests (pure logic) can't. The Rust backend is mocked at the invoke
// boundary: every command returns canned data, so no window is needed.

// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
// fireEvent over userEvent: the shell's transitions are simple click flows and
// the shorter API keeps this smoke test focused on navigation, not typing.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// jsdom does not implement Element.prototype.scrollTo (a real browser does).
// The chat screen auto-scrolls on updates; give the smoke test a no-op shim
// so the effect doesn't crash the render — this is an environment gap, not
// an app bug (WebView2 always provides scrollTo).
if (typeof HTMLElement.prototype.scrollTo !== "function") {
  HTMLElement.prototype.scrollTo = () => {}
}

// Mock the Tauri bridge before the app modules import it: invoke() becomes a
// scriptable stub returning canned rows, and listen() resolves an unlisten
// handle so the chat screen's subscription effect doesn't hang.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

import { App } from "./App"
import { useCampaignStore } from "./modules/campaigns"
import { useChatStore } from "./modules/chat"
import { invoke } from "@tauri-apps/api/core"

// A fixed campaign the mocked backend returns for both the list and the
// single-campaign lookup, so the top bar shows a real name in the chat stage.
const campaign = {
  id: "c1",
  name: "The Duskmoor Pact",
  description: "A blood oath under a dying star.",
  ruleset_id: null,
  settings: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-02T00:00:00Z",
}

// Render the shell with a fresh query cache so TanStack Query starts clean.
function renderApp() {
  // One cache per render: retry defaults stay off to keep the test fast.
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>,
  )
}

// Script the invoke stub: each Tauri command returns its canned response so
// the screens never see a rejected call.
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>
function stubBackend() {
  invokeMock.mockImplementation((command: string) => {
    switch (command) {
      case "list_campaigns":
        return Promise.resolve([campaign])
      case "get_campaign":
        return Promise.resolve(campaign)
      case "list_messages":
        return Promise.resolve([])
      default:
        return Promise.resolve(null)
    }
  })
}

describe("App stage flow", () => {
  beforeEach(() => {
    // Reset both stores to pristine state and re-stub the backend between
    // cases — Zustand stores and the invoke mock are module singletons.
    useChatStore.setState({ byCampaign: {}, drafts: {}, streaming: {}, activity: {}, errors: {} })
    useCampaignStore.setState({ activeCampaignId: null, campaignsVersion: 0 })
    stubBackend()
  })

  it("lands on the lobby and opens a campaign's chat", async () => {
    renderApp()
    // findByText waits for the list query to resolve past the loading
    // spinner — the first synchronous render is always "Loading campaigns…".
    expect(await screen.findByText("Roleplayer")).toBeInTheDocument()
    // The mocked campaign row is listed with its name.
    expect(screen.getByText("The Duskmoor Pact")).toBeInTheDocument()

    // Clicking the row's open target navigates into the chat stage.
    fireEvent.click(screen.getByText("The Duskmoor Pact"))
    // The composer placeholder confirms the chat screen rendered.
    expect(await screen.findByPlaceholderText("Describe your action…")).toBeInTheDocument()
    // The chat top bar shows the campaign name; findByText waits for the
    // separate get_campaign query to resolve (the lobby is unmounted now).
    expect(await screen.findByText("The Duskmoor Pact")).toBeInTheDocument()
    // The Action/Speech mode toggle is part of the composer.
    expect(screen.getByRole("group", { name: "Message mode" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Action" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Speech" })).toBeInTheDocument()
  })

  it("hides the world/data panels behind the debug drawer", async () => {
    renderApp()
    // Open the campaign so the chat stage (and its top bar) is visible.
    fireEvent.click(await screen.findByText("The Duskmoor Pact"))
    await screen.findByPlaceholderText("Describe your action…")

    // The debug panels must NOT be visible before the toggle is pressed.
    expect(screen.queryByText("World state (0)")).not.toBeInTheDocument()

    // The top-bar Debug toggle opens the drawer with its section tabs.
    fireEvent.click(screen.getByLabelText("Toggle debug panel"))
    expect(await screen.findByRole("dialog", { name: "Debug panel" })).toBeInTheDocument()
    // The drawer's own heading and its secondary tabs render inside.
    expect(screen.getByText("Debug")).toBeInTheDocument()
    expect(screen.getByRole("tab", { name: "World" })).toBeInTheDocument()

    // Esc dismisses the drawer (standard overlay behavior, shell-handled).
    fireEvent.keyDown(window, { key: "Escape" })
    expect(screen.queryByRole("dialog", { name: "Debug panel" })).not.toBeInTheDocument()
  })

  it("returns from the chat to the lobby via the back button", async () => {
    renderApp()
    fireEvent.click(await screen.findByText("The Duskmoor Pact"))
    await screen.findByPlaceholderText("Describe your action…")

    // The back chevron is the single step out of a campaign's chat.
    fireEvent.click(screen.getByLabelText("Back to campaigns"))
    // The lobby is visible again, with the title and campaign row restored.
    expect(screen.getByText("Roleplayer")).toBeInTheDocument()
    expect(screen.getByText("The Duskmoor Pact")).toBeInTheDocument()
  })
})
