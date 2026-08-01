// Debug window root: the pop-out window that holds the world / characters /
// memories / search panels with room to breathe.
//
// This is NOT a stage of the main app — it is a separate Tauri window loaded
// as `index.html#/debug/<campaign_id>`. main.tsx sees the hash and mounts this
// root instead of App. It reuses the module-owned panels at full width, which
// is exactly what a 400px in-app drawer could not fit.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { getCurrentWindow } from "@tauri-apps/api/window"

import { getCampaign } from "./modules/campaigns"
import { CharactersPanel } from "./modules/characters"
import { WorldPanel } from "./modules/world"
import { MemoriesPanel } from "./modules/memories"
import { SearchPanel } from "./modules/search"

// The four debug sections; a full-width tab strip, not the cramped drawer tabs.
type DebugTab = "world" | "characters" | "memories" | "search"

export function DebugRoot({ campaignId }: { campaignId: string }) {
  // Which section fills the window body; local state, resets on reopen.
  const [tab, setTab] = useState<DebugTab>("world")
  // The campaign name makes the window title readable on its own.
  const { data: campaign } = useQuery({
    queryKey: ["campaign", campaignId],
    queryFn: () => getCampaign(campaignId),
  })

  // Close this window. Only reachable inside a real Tauri window (this root
  // is only mounted by the debug-window route). The promise can reject (e.g.
  // the window is already closing), so swallow it rather than surface an
  // unhandled rejection (§5.10).
  function closeWindow() {
    getCurrentWindow()
      .close()
      .catch(() => {})
  }

  return (
    <div className="stage">
      {/* Same 48px top bar language as the main app, with a close button. */}
      <header className="topbar">
        <span className="topbar-title">Debug — {campaign?.name ?? campaignId}</span>
        <button className="icon-btn" onClick={closeWindow} aria-label="Close debug window" title="Close">
          ×
        </button>
      </header>

      <div className="home">
        <div className="home-inner">
          {/* Full-width section tabs; the accent marks the active section. */}
          <div className="row">
            {(
              [
                { id: "world", label: "World" },
                { id: "characters", label: "Characters" },
                { id: "memories", label: "Memories" },
                { id: "search", label: "Search" },
              ] as const
            ).map((item) => (
              <button
                key={item.id}
                className={`btn ${tab === item.id ? "btn-primary" : ""}`}
                onClick={() => setTab(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>

          {/* Each section renders the module's own panel at full window width. */}
          {tab === "world" ? (
            <WorldPanel campaignId={campaignId} />
          ) : tab === "characters" ? (
            <CharactersPanel campaignId={campaignId} />
          ) : tab === "memories" ? (
            <MemoriesPanel campaignId={campaignId} />
          ) : (
            // Fallback: any future tab id defaults to search rather than blank.
            <SearchPanel campaignId={campaignId} />
          )}
        </div>
      </div>
    </div>
  )
}
