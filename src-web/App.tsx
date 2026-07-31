// App shell: sidebar navigation + the three top-level views (campaigns,
// campaign workspace, settings). Kept intentionally thin — screens live in
// their feature modules; this file only composes them.

// Local state is the only React state the shell needs — the rest lives in
// module stores (campaign selection) and TanStack Query (server data).
import { useState } from "react"

// Each import is the module's public surface (index.ts); internals stay out
// of reach so App composes modules without touching their private files.
import { useCampaignStore } from "./modules/campaigns"
import { CampaignsScreen } from "./modules/campaigns"
import { ChatScreen } from "./modules/chat"
import { CharactersPanel } from "./modules/characters"
import { WorldPanel } from "./modules/world"
import { MemoriesPanel } from "./modules/memories"
import { SearchPanel } from "./modules/search"
import { ProvidersScreen } from "./modules/providers"
import { RulesetsScreen } from "./modules/rulesets"

// Two-level navigation: a coarse top-level View (campaign list / workspace /
// settings) plus a tab bar inside the workspace. Plain string unions because
// the only consumer is the conditional render below.
// Top-level route: which screen family is showing.
type View = "campaigns" | "campaign" | "settings"
// Workspace-level tab: which module panel fills the campaign view.
type WorkspaceTab = "chat" | "world" | "characters" | "memories" | "search"

export function App() {
  // Start on the campaign list — the app's landing view on every launch.
  const [view, setView] = useState<View>("campaigns")
  // The store owns "which campaign is open" so the sidebar's "Resume" button
  // and the workspace stay in sync without prop drilling.
  // Subscribing via selector re-renders the shell whenever this id changes.
  const activeCampaignId = useCampaignStore((state) => state.activeCampaignId)

  function openCampaign(campaignId: string) {
    // Record the selection in the store first, then flip the view; the
    // workspace reads the id from the store on the next render.
    // Two writes in one handler: the store id must land before the view flips
    // or the workspace would mount with a null id for one render frame.
    useCampaignStore.getState().setActiveCampaign(campaignId)
    setView("campaign")
  }

  function backToCampaigns() {
    // Clearing the active campaign is what unmounts the workspace; leaving a
    // stale id would make the sidebar "Resume" button lie about state.
    // Unmounting also drops the chat event subscription via effect cleanup.
    useCampaignStore.getState().setActiveCampaign(null)
    setView("campaigns")
  }

  return (
    <div className="app-shell">
      {/* Fixed nav rail; everything inside is just view switching. */}
      <nav className="sidebar" aria-label="Main navigation">
        <h1 className="sidebar-title">Roleplayer</h1>

        {/* Always reachable: the list is the app's home base. */}
        <button className="btn btn-ghost" onClick={backToCampaigns}>
          ← Campaigns
        </button>
        {/* Settings is a top-level view, not a workspace tab. */}
        <button className="btn btn-ghost" onClick={() => setView("settings")}>
          Settings
        </button>

        {activeCampaignId ? (
          <>
            {/* Resume only exists while a campaign is selected; the section
                header groups it visually under the app title. */}
            <p className="sidebar-section">Active</p>
            {/* Jumping back to the workspace is the fastest re-entry path;
                the id is still in the store so no refetch is needed. */}
            <button className="btn btn-ghost" onClick={() => setView("campaign")}>
              ▶ Resume campaign
            </button>
          </>
        ) : null}
      </nav>

      <main className="main">
        <div className="main-scroll">
          {/* Settings wins over the workspace; a campaign view only renders
              while an id is actually active, otherwise fall back to the list. */}
          {view === "settings" ? (
            <SettingsTabs />
          ) : view === "campaign" && activeCampaignId ? (
            <CampaignWorkspace campaignId={activeCampaignId} />
          ) : (
            <CampaignsScreen onOpenCampaign={openCampaign} />
          )}
        </div>
      </main>
    </div>
  )
}

/** The per-campaign workspace: chat + world state + characters + memories +
 *  search, switched via a tab bar. */
function CampaignWorkspace({ campaignId }: { campaignId: string }) {
  // Tab selection is local state; the workspace remounts per campaign, so no
  // tab persistence is needed (each campaign starts on Chat).
  // Reset on remount also means a reopened campaign always lands on Chat.
  const [tab, setTab] = useState<WorkspaceTab>("chat")

  // Static tab registry — order here defines the tab bar order in the UI.
  const tabs: { id: WorkspaceTab; label: string }[] = [
    { id: "chat", label: "Chat" },
    { id: "world", label: "World" },
    { id: "characters", label: "Characters" },
    { id: "memories", label: "Memories" },
    { id: "search", label: "Search" },
  ]

  return (
    // Height 100% pins the panel so its own scroll areas (chat transcript)
    // manage scrolling instead of the outer page.
    <div className="col" style={{ height: "100%" }}>
      <div className="row" style={{ flexWrap: "wrap" }}>
        {/* Render one button per tab; the active tab gets the primary tint. */}
        {tabs.map((item) => (
          <button
            key={item.id}
            className={`btn ${tab === item.id ? "btn-primary" : ""}`}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      {/* Composition point: each tab renders a module-owned panel so App.tsx
          stays thin and modules own their screens end-to-end. */}
      {tab === "chat" ? (
        // Chat owns the streaming event subscription for this campaign.
        <ChatScreen campaignId={campaignId} />
      ) : tab === "world" ? (
        <WorldPanel campaignId={campaignId} />
      ) : tab === "characters" ? (
        <CharactersPanel campaignId={campaignId} />
      ) : tab === "memories" ? (
        <MemoriesPanel campaignId={campaignId} />
      ) : (
        // Fallback: any future tab id defaults to search rather than a blank.
        <SearchPanel campaignId={campaignId} />
      )}
    </div>
  )
}

function SettingsTabs() {
  // Providers is the default settings tab; the conditional below unmounts
  // the inactive screen, so each tab re-mounts with fresh state when visited.
  const [tab, setTab] = useState<"providers" | "rulesets">("providers")
  return (
    <div className="col">
      <div className="row">
        {/* Highlight the selected settings tab like the workspace tabs do. */}
        <button className={`btn ${tab === "providers" ? "btn-primary" : ""}`} onClick={() => setTab("providers")}>
          Providers
        </button>
        <button className={`btn ${tab === "rulesets" ? "btn-primary" : ""}`} onClick={() => setTab("rulesets")}>
          Rulesets
        </button>
      </div>
      {/* Conditional render swaps the settings pane; the ternary is the whole
          tab logic since there are exactly two options. */}
      {tab === "providers" ? <ProvidersScreen /> : <RulesetsScreen />}
    </div>
  )
}
