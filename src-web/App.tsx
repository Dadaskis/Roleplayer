// App shell: sidebar navigation + the three top-level views (campaigns,
// campaign workspace, settings). Kept intentionally thin — screens live in
// their feature modules; this file only composes them.

import { useState } from "react"

import { useCampaignStore } from "./modules/campaigns"
import { CampaignsScreen } from "./modules/campaigns"
import { ChatScreen } from "./modules/chat"
import { CharactersPanel } from "./modules/characters"
import { WorldPanel } from "./modules/world"
import { MemoriesPanel } from "./modules/memories"
import { SearchPanel } from "./modules/search"
import { ProvidersScreen } from "./modules/providers"
import { RulesetsScreen } from "./modules/rulesets"

type View = "campaigns" | "campaign" | "settings"
type WorkspaceTab = "chat" | "world" | "characters" | "memories" | "search"

export function App() {
  const [view, setView] = useState<View>("campaigns")
  const activeCampaignId = useCampaignStore((state) => state.activeCampaignId)

  function openCampaign(campaignId: string) {
    useCampaignStore.getState().setActiveCampaign(campaignId)
    setView("campaign")
  }

  function backToCampaigns() {
    useCampaignStore.getState().setActiveCampaign(null)
    setView("campaigns")
  }

  return (
    <div className="app-shell">
      <nav className="sidebar" aria-label="Main navigation">
        <h1 className="sidebar-title">Roleplayer</h1>

        <button className="btn btn-ghost" onClick={backToCampaigns}>
          ← Campaigns
        </button>
        <button className="btn btn-ghost" onClick={() => setView("settings")}>
          Settings
        </button>

        {activeCampaignId ? (
          <>
            <p className="sidebar-section">Active</p>
            <button className="btn btn-ghost" onClick={() => setView("campaign")}>
              ▶ Resume campaign
            </button>
          </>
        ) : null}
      </nav>

      <main className="main">
        <div className="main-scroll">
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
  const [tab, setTab] = useState<WorkspaceTab>("chat")

  const tabs: { id: WorkspaceTab; label: string }[] = [
    { id: "chat", label: "Chat" },
    { id: "world", label: "World" },
    { id: "characters", label: "Characters" },
    { id: "memories", label: "Memories" },
    { id: "search", label: "Search" },
  ]

  return (
    <div className="col" style={{ height: "100%" }}>
      <div className="row" style={{ flexWrap: "wrap" }}>
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

      {tab === "chat" ? (
        <ChatScreen campaignId={campaignId} />
      ) : tab === "world" ? (
        <WorldPanel campaignId={campaignId} />
      ) : tab === "characters" ? (
        <CharactersPanel campaignId={campaignId} />
      ) : tab === "memories" ? (
        <MemoriesPanel campaignId={campaignId} />
      ) : (
        <SearchPanel campaignId={campaignId} />
      )}
    </div>
  )
}

function SettingsTabs() {
  const [tab, setTab] = useState<"providers" | "rulesets">("providers")
  return (
    <div className="col">
      <div className="row">
        <button className={`btn ${tab === "providers" ? "btn-primary" : ""}`} onClick={() => setTab("providers")}>
          Providers
        </button>
        <button className={`btn ${tab === "rulesets" ? "btn-primary" : ""}`} onClick={() => setTab("rulesets")}>
          Rulesets
        </button>
      </div>
      {tab === "providers" ? <ProvidersScreen /> : <RulesetsScreen />}
    </div>
  )
}
