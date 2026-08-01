// App shell: a stage machine — the app is one full-screen scene at a time.
//
// Stages (see AGENTS.md §4 / the design notes in PLAN.md §5):
//   "home"     → campaign lobby: pick or create a campaign (the landing).
//   "chat"     → the campaign's chat, full-window. The story owns the screen.
//   "settings" → providers + rulesets, reached from the top-bar gear.
//
// There is deliberately NO permanent sidebar or back stack: "back" is a single
// step from chat to the lobby, and from settings back to wherever it opened.
// Everything extra (world state, memories, audit, search) lives in a separate
// pop-out debug window (see DebugRoot) — hidden from the player by default.

// Local state is the only React state the shell needs — module stores hold the
// rest (campaign selection) and TanStack Query owns server data.
import { useEffect, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

// The typed IPC bridge for the shell-level debug-window command.
import { onTurnEvent, openDebugWindow } from "./core/api/invoke"
import { Button } from "./core/ui/components"

// Each import is the module's public surface (index.ts); internals stay out of
// reach so App composes modules without touching their private files.
import { CampaignsScreen, getCampaign, useCampaignStore } from "./modules/campaigns"
import { ChatScreen, startRoleplay } from "./modules/chat"
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

  // The three navigation verbs; each is a single step — no history stack.
  function openCampaign(campaignId: string) {
    // Record the selection in the store first; the chat reads it next render.
    // Two writes in one handler: the store id must land before the stage flips
    // or the chat would mount with a null id for one render frame.
    useCampaignStore.getState().setActiveCampaign(campaignId)
    setStage("chat")
  }

  function backToHome() {
    // Clearing the active campaign unmounts the chat (and its event
    // subscription via effect cleanup) and makes the lobby the truth again.
    useCampaignStore.getState().setActiveCampaign(null)
    setStage("home")
  }

  function openSettings() {
    // Remember where we came from so back returns there, not always home.
    setSettingsFrom(stage)
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
    />
  ) : (
    <div className="stage">
      <CampaignsScreen onOpenCampaign={openCampaign} onOpenSettings={openSettings} />
    </div>
  )
}

/** The chat stage: a 48px top bar (back, campaign name, Debug + Settings)
 *  over the full-window chat. The Debug button opens the pop-out window. */
function ChatStage({
  campaignId,
  onBack,
  onOpenSettings,
}: {
  campaignId: string
  onBack: () => void
  onOpenSettings: () => void
}) {
  // The campaign name comes from a dedicated query (not the lobby cache) so
  // the top bar reads correctly even if this chat was entered before a list
  // fetch; the lobby's list and this lookup share the same backend row.
  const queryClient = useQueryClient()
  const { data: campaign } = useQuery({
    queryKey: ["campaign", campaignId],
    queryFn: () => getCampaign(campaignId),
  })

  // Keep the campaign's status fresh: after the worldgen turn completes (or
  // fails), the status flips worldgen → active/setup. Refetch the campaign on
  // turn_complete/turn_error so the Start-roleplay button reflects reality.
  useEffect(() => {
    let unlisten: (() => void) | null = null
    let active = true
    onTurnEvent((event) => {
      if (event.campaign_id !== campaignId) {
        return
      }
      if (event.type === "turn_complete" || event.type === "turn_error") {
        queryClient.invalidateQueries({ queryKey: ["campaign", campaignId] })
      }
    }).then((release) => {
      if (active) {
        unlisten = release
      } else {
        release()
      }
    })
    return () => {
      active = false
      unlisten?.()
    }
  }, [campaignId, queryClient])

  // The lifecycle status drives the Start-roleplay affordance: setup offers
  // the button, worldgen shows a locked "generating" state, active hides both.
  const status = campaign?.status
  // Local pending flag: while a start request is in flight the button locks,
  // so a double-click can't fire two start_roleplay calls (the backend would
  // reject the second anyway; this keeps the UI honest in the meantime).
  const [starting, setStarting] = useState(false)

  function handleStartRoleplay() {
    setStarting(true)
    startRoleplay(campaignId).catch(() => {
      // A rejected start (e.g. the campaign left setup between render and
      // click) must unlock the button rather than leave it frozen.
      setStarting(false)
    })
  }

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
          {/* Start-roleplay affordance: the player ends the setup Q&A and the
              GM generates the world + characters, then opens the story. */}
          {status === "setup" ? (
            <Button variant="primary" onClick={handleStartRoleplay} disabled={starting}>
              {starting ? "Generating…" : "Start roleplay"}
            </Button>
          ) : status === "worldgen" ? (
            // The generation turn is single-flight; the button reads as a
            // locked state until the campaign settles active or back to setup.
            <Button variant="ghost" disabled>
              Generating world…
            </Button>
          ) : null}
          {/* Debug opens the pop-out window with the world/memory/audit data.
              Labeled plainly "Debug" because that is exactly what it reveals —
              raw data under the hood, not a feature. */}
          <button
            className="icon-btn"
            onClick={() => openDebugWindow(campaignId)}
            aria-label="Open debug window"
            title="Open debug window (world state, memories, audit)"
          >
            {"</>"}
          </button>
          {/* Settings gear opens the settings stage from the chat. */}
          <button className="icon-btn" onClick={onOpenSettings} aria-label="Settings" title="Settings">
            ⚙
          </button>
        </div>
      </header>

      {/* The chat fills everything under the bar; the campaign phase lets it
          auto-kick the setup intro when a fresh setup campaign opens. */}
      <ChatScreen campaignId={campaignId} status={status} />
    </div>
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
