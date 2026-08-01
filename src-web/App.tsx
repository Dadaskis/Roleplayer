// App shell: a stage machine — the app is one full-screen scene at a time.
//
// Stages (see AGENTS.md §4 / the design notes in PLAN.md §5):
//   "home"     → campaign lobby: pick or create a campaign (the landing).
//   "chat"     → the campaign's chat, full-window. The story owns the screen.
//   "settings" → providers + rulesets, reached from the top-bar gear.
//
// There is deliberately NO permanent sidebar or back stack: "back" is a single
// step from chat to the lobby, and from settings back to wherever it opened.
// Everything extra (world state, memories, audit, search) lives behind the
// Debug toggle in a right-hand drawer — hidden from the player by default.

// Local state is the only React state the shell needs — module stores hold the
// rest (campaign selection) and TanStack Query owns server data.
import { useEffect, useRef, useState } from "react"
import { useQuery } from "@tanstack/react-query"

// Each import is the module's public surface (index.ts); internals stay out of
// reach so App composes modules without touching their private files.
import { CampaignsScreen, getCampaign, useCampaignStore } from "./modules/campaigns"
import { ChatScreen } from "./modules/chat"
import { CharactersPanel } from "./modules/characters"
import { WorldPanel } from "./modules/world"
import { MemoriesPanel } from "./modules/memories"
import { SearchPanel } from "./modules/search"
import { ProvidersScreen } from "./modules/providers"
import { RulesetsScreen } from "./modules/rulesets"

// The three full-screen scenes; "chat" only renders while a campaign is active.
type Stage = "home" | "chat" | "settings"

export function App() {
  // Start on the campaign lobby — the app's landing view on every launch.
  const [stage, setStage] = useState<Stage>("home")
  // The store owns "which campaign is open" so the lobby and the chat stay in
  // sync without prop drilling. Subscribing via selector re-renders on change.
  const activeCampaignId = useCampaignStore((state) => state.activeCampaignId)
  // Where Settings opened from, so its back button returns to the right stage.
  const [settingsFrom, setSettingsFrom] = useState<Stage>("home")
  // The Debug drawer is shell-level state: the top bar toggles it and the
  // drawer component only reports close/escape events back up. Deliberately
  // not persisted — a returning user should never see raw data by surprise.
  const [debugOpen, setDebugOpen] = useState(false)

  // Shell-level keyboard shortcuts: Esc closes the drawer, Ctrl/Cmd+D toggles
  // it. Kept here so no single screen owns a shortcut the whole app uses.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      // Esc always dismisses the drawer when it is open (standard overlay UX).
      if (event.key === "Escape" && debugOpen) {
        setDebugOpen(false)
        return
      }
      // Ctrl/Cmd+D toggles the debug drawer from anywhere (power-user affordance).
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "d") {
        event.preventDefault()
        setDebugOpen((open) => !open)
      }
    }
    // Global keydown (not the focused element) so shortcuts work mid-typing too.
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [debugOpen])

  // The three navigation verbs; each is a single step — no history stack.
  function openCampaign(campaignId: string) {
    // Record the selection in the store first; the chat reads it next render.
    // Two writes in one handler: the store id must land before the stage flips
    // or the chat would mount with a null id for one render frame.
    useCampaignStore.getState().setActiveCampaign(campaignId)
    // A fresh chat must never open with a stale drawer from a previous visit.
    setDebugOpen(false)
    setStage("chat")
  }

  function backToHome() {
    // Clearing the active campaign unmounts the chat (and its event
    // subscription via effect cleanup) and makes the lobby the truth again.
    useCampaignStore.getState().setActiveCampaign(null)
    setDebugOpen(false)
    setStage("home")
  }

  function openSettings() {
    // Remember where we came from so back returns there, not always home.
    setSettingsFrom(stage)
    // Close any open drawer; Settings is a full stage, not an overlay.
    setDebugOpen(false)
    setStage("settings")
  }

  // Stage render: exactly one scene at a time. Settings is a full stage so it
  // wins over the chat; the chat only renders while a campaign is actually
  // active, otherwise the lobby shows (covers the "cleared store" case).
  return stage === "settings" ? (
    <SettingsStage onBack={() => setStage(settingsFrom)} />
  ) : stage === "chat" && activeCampaignId ? (
    <ChatStage
      campaignId={activeCampaignId}
      onBack={backToHome}
      onOpenSettings={openSettings}
      debugOpen={debugOpen}
      onToggleDebug={() => setDebugOpen((open) => !open)}
      onCloseDebug={() => setDebugOpen(false)}
    />
  ) : (
    <div className="stage">
      <CampaignsScreen onOpenCampaign={openCampaign} onOpenSettings={openSettings} />
    </div>
  )
}

/** The chat stage: a 48px top bar (back, campaign name, Debug + Settings)
 *  over the full-window chat, with the debug drawer layered on top when open. */
function ChatStage({
  campaignId,
  onBack,
  onOpenSettings,
  debugOpen,
  onToggleDebug,
  onCloseDebug,
}: {
  campaignId: string
  onBack: () => void
  onOpenSettings: () => void
  debugOpen: boolean
  onToggleDebug: () => void
  onCloseDebug: () => void
}) {
  // The campaign name comes from a dedicated query (not the lobby cache) so
  // the top bar reads correctly even if this chat was entered before a list
  // fetch; the lobby's list and this lookup share the same backend row.
  const { data: campaign } = useQuery({
    queryKey: ["campaign", campaignId],
    queryFn: () => getCampaign(campaignId),
  })

  return (
    <div className="stage">
      {/* The only chrome on the chat stage: identity left, actions right. */}
      <header className="topbar">
        <div className="row">
          {/* The one explicit "back" affordance — a single step to the lobby.
              No history, no breadcrumbs: two stages don't need a stack. */}
          <button className="icon-btn" onClick={onBack} aria-label="Back to campaigns" title="Back to campaigns">
            ‹
          </button>
          {/* Campaign name is the bar's identity; absent while loading. */}
          <span className="topbar-title">{campaign?.name ?? ""}</span>
        </div>
        <div className="row">
          {/* Debug toggle: accent-marked while the drawer is open. Labeled
              plainly "Debug" because that is exactly what it reveals — raw
              world/memory/audit data under the hood, not a feature. */}
          <button
            className={`icon-btn ${debugOpen ? "icon-btn-active" : ""}`}
            onClick={onToggleDebug}
            aria-label="Toggle debug panel"
            aria-pressed={debugOpen}
            title="Debug panel (world state, memories, audit)"
          >
            {"</>"}
          </button>
          {/* Settings gear opens the settings stage from the chat. */}
          <button className="icon-btn" onClick={onOpenSettings} aria-label="Settings" title="Settings">
            ⚙
          </button>
        </div>
      </header>

      {/* The chat fills everything under the bar; it never shrinks to make
          room for the drawer — the drawer overlays it instead. */}
      <ChatScreen campaignId={campaignId} />

      {/* Conditional drawer: only mounted while open, so its focus is trapped
          and released cleanly on close (see DebugDrawer). */}
      {debugOpen ? <DebugDrawer campaignId={campaignId} onClose={onCloseDebug} /> : null}
    </div>
  )
}

/** The debug drawer: a right slide-in overlay holding the world/characters/
 *  memories/search panels behind a secondary tab strip. It is developer-facing
 *  reference material to glance at while chatting — never a modal. */
function DebugDrawer({ campaignId, onClose }: { campaignId: string; onClose: () => void }) {
  // Which sub-panel the drawer shows; local state, resets when reopened.
  const [tab, setTab] = useState<"world" | "characters" | "memories" | "search">("world")
  // The drawer root; used by the focus trap to collect focusable elements.
  const drawerRef = useRef<HTMLDivElement>(null)
  // The close button; focused on open so keyboard users land on an escape.
  const closeRef = useRef<HTMLButtonElement>(null)

  // Focus the close button on mount so the drawer opens with a keyboard target.
  useEffect(() => {
    closeRef.current?.focus()
  }, [])

  // Trap Tab/Shift+Tab inside the drawer (accessibility requirement for
  // overlays): without this, focus would escape into the chat behind it.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      // Only the Tab key matters here — Esc is handled at the shell level.
      if (event.key !== "Tab") {
        return
      }
      // Collect every focusable element currently inside the drawer.
      const root = drawerRef.current
      if (!root) {
        return
      }
      // Standard focusable selectors; input type=hidden is excluded by :not.
      const focusable = root.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) {
        // No focusable targets: swallow Tab so it can't leak to the page.
        event.preventDefault()
        return
      }
      // First and last elements define the wrap-around for a circular trap.
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      // Shift+Tab from the first element wraps to the last…
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
        // …and plain Tab from the last wraps back to the first.
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  return (
    <>
      {/* Backdrop dims the chat and acts as the click-away close target. */}
      <div className="drawer-backdrop" onClick={onClose} aria-hidden="true" />
      {/* The panel itself; role=dialog + aria-modal marks it as an overlay. */}
      <div className="drawer" ref={drawerRef} role="dialog" aria-label="Debug panel">
        <div className="drawer-header">
          <span className="drawer-title">Debug</span>
          <button ref={closeRef} className="icon-btn" onClick={onClose} aria-label="Close debug panel">
            ×
          </button>
        </div>

        {/* Secondary tab strip: the old workspace tabs, demoted to drawer
            selectors — they never compete with the chat now. */}
        <div className="drawer-tabs" role="tablist" aria-label="Debug sections">
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
              role="tab"
              aria-selected={tab === item.id}
              className={`drawer-tab ${tab === item.id ? "drawer-tab-active" : ""}`}
              onClick={() => setTab(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>

        {/* Each tab renders the module's own panel inside the drawer body;
            App stays thin and the panels stay owned end-to-end by modules. */}
        <div className="drawer-body">
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
    </>
  )
}

/** The settings stage: providers + rulesets behind a two-tab strip, with a
 *  back button that returns to the stage the user opened it from. */
function SettingsStage({ onBack }: { onBack: () => void }) {
  // Providers is the default settings tab; the conditional below unmounts the
  // inactive screen, so each tab re-mounts with fresh state when visited.
  const [tab, setTab] = useState<"providers" | "rulesets">("providers")
  return (
    <div className="stage">
      {/* Settings gets its own slim top bar so the back affordance is present
          and consistent with the chat stage's navigation language. */}
      <header className="topbar">
        <div className="row">
          {/* Returns to wherever Settings was opened from (home or chat). */}
          <button className="icon-btn" onClick={onBack} aria-label="Back" title="Back">
            ‹
          </button>
          <span className="topbar-title">Settings</span>
        </div>
      </header>
      <div className="home">
        <div className="home-inner">
          <div className="row">
            {/* Highlight the selected settings tab like a segmented control. */}
            <button className={`btn ${tab === "providers" ? "btn-primary" : ""}`} onClick={() => setTab("providers")}>
              Providers
            </button>
            <button className={`btn ${tab === "rulesets" ? "btn-primary" : ""}`} onClick={() => setTab("rulesets")}>
              Rulesets
            </button>
          </div>
          {/* Conditional render swaps the settings pane; the ternary is the
              whole tab logic since there are exactly two options. */}
          {tab === "providers" ? <ProvidersScreen /> : <RulesetsScreen />}
        </div>
      </div>
    </div>
  )
}
