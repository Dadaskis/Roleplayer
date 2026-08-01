// Campaign lobby screen (the app's landing view).
//
// This is "stage 1" of the stage model: a focused lobby where the user picks a
// campaign (or creates one) before entering its chat. There is no sidebar and
// no workspace chrome here — the lobby is a quiet list, and "New roleplay" is
// a secondary action behind a toggle, not a billboard.

// useQuery/useQueryClient own the server-data cache for this module.
import { useQuery, useQueryClient } from "@tanstack/react-query"
// Local form state only; server data never lives in component state here.
import { useState } from "react"

// Shared UI primitives and the one normalized error-message helper.
import { Button, Card, Input, Spinner, Textarea } from "../../core/ui/components"
import { errorMessage } from "../../core/api/invoke"
// Typed IPC wrappers — the only way this screen talks to the backend.
import { createCampaign, deleteCampaign, listCampaigns } from "./api"
import { useCampaignStore } from "./store"
import type { Campaign } from "./types"

export function CampaignsScreen({
  onOpenCampaign,
  onOpenSettings,
}: {
  onOpenCampaign: (campaignId: string) => void
  onOpenSettings: () => void
}) {
  // The cache handle lets this screen invalidate the list after mutations.
  const queryClient = useQueryClient()
  // Store read for "which campaign is open", used to clear it on delete.
  const { activeCampaignId, setActiveCampaign } = useCampaignStore()
  // The create form is hidden behind a toggle: new campaigns are rare next to
  // resuming an existing story, so the form is opt-in, not always visible.
  const [showCreate, setShowCreate] = useState(false)
  // Create-form fields: local state is fine, the form resets after submit.
  const [name, setName] = useState("")
  const [description, setDescription] = useState("")
  // Guards the submit button from double-clicks while a create is in flight.
  const [creating, setCreating] = useState(false)
  // Last create/delete failure, shown as a danger badge above the form.
  const [error, setError] = useState<string | null>(null)

  // A single stable query key for the whole list — any mutation (create /
  // delete) invalidates it and every subscriber refetches together.
  const { data: campaigns, isLoading } = useQuery({
    queryKey: ["campaigns"],
    // Passing the fn directly: listCampaigns takes no args, matches the
    // signature useQuery expects, so no wrapper closure is needed.
    queryFn: listCampaigns,
  })

  // Invalidating the query (not setting local state) keeps this screen
  // consistent with any other consumer of the same key.
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["campaigns"] })

  // Sort by most-recently-updated first so the story the user was just in
  // surfaces at the top — a lobby should feel like "continue where I left".
  // Copy-then-sort: listCampaigns data is never mutated in place.
  const sorted = campaigns ? [...campaigns].sort((a, b) => b.updated_at.localeCompare(a.updated_at)) : []

  async function handleCreate() {
    // Guard the boundary: reject blank names before they reach the backend,
    // and clear any previous error so a retry doesn't show stale feedback.
    if (!name.trim()) {
      setError("A campaign needs a name.")
      return
    }
    // Lock the button so a double-submit can't create two campaigns.
    setCreating(true)
    setError(null)
    try {
      const campaign = await createCampaign({ name, description, ruleset_id: null })
      // Clear the form and hide it, then jump straight into the new campaign
      // so the user can start roleplaying instead of hunting it in the list.
      setName("")
      setDescription("")
      setShowCreate(false)
      // Refetch the list so the new row appears under both this screen and
      // the store's campaignsVersion consumers on their next read.
      refresh()
      // Hand the new id up; App flips to the chat stage on this call.
      onOpenCampaign(campaign.id)
    } catch (reason) {
      // Surface the backend's structured message; the form stays filled so a
      // retry is a single click, not retyping.
      setError(errorMessage(reason))
    } finally {
      // Always unlock, success or failure — leaving it locked would freeze
      // the form until a page reload.
      setCreating(false)
    }
  }

  async function handleDelete(campaign: Campaign) {
    // Destructive ops are gated behind explicit confirmation (§5.16); deleting
    // a campaign drops all its messages, so this is a hard stop for the user.
    if (!confirm(`Delete "${campaign.name}" and all of its data? This cannot be undone.`)) {
      return
    }
    await deleteCampaign(campaign.id)
    // If the deleted campaign was the open chat, clear the store too so the
    // app doesn't point at a now-gone id.
    if (activeCampaignId === campaign.id) {
      setActiveCampaign(null)
    }
    // After a delete the list must refetch; the UI decides this is needed
    // unconditionally since the backend already told us a row was removed.
    refresh()
  }

  // First paint: the list has not arrived yet, show the spinner immediately.
  if (isLoading) {
    return (
      <div className="home">
        <div className="home-inner">
          <Spinner label="Loading campaigns..." />
        </div>
      </div>
    )
  }

  return (
    <div className="home">
      <div className="home-inner">
        {/* Header: the brand title left, the Settings gear right — the only
            two things the lobby's header needs. */}
        <div className="home-header">
          <h1 className="home-title">Roleplayer</h1>
          <button className="icon-btn" onClick={onOpenSettings} aria-label="Settings" title="Settings">
            ⚙
          </button>
        </div>

        {/* The campaign list: the lobby's main content, newest activity first.
            The "+ New roleplay" toggle sits on the right of the section label,
            so creating stays a secondary action beside the list it feeds. */}
        <div className="row" style={{ justifyContent: "space-between" }}>
          {/* .home-section already carries margin: 0; no inline override. */}
          <p className="home-section">Your roleplays ({sorted.length})</p>
          <button
            className="btn btn-ghost"
            onClick={() => {
              // Toggle the form; a fresh open clears any stale error so the
              // failure from a previous attempt doesn't linger behind it.
              setShowCreate((visible) => !visible)
              setError(null)
            }}
          >
            {showCreate ? "Cancel" : "+ New roleplay"}
          </button>
        </div>

        {/* The create form, only rendered while toggled open. */}
        {showCreate ? (
          <Card>
            <div className="col">
              {/* Controlled input: every keystroke updates `name` so the
                  submit guard and the disabled state both read latest. */}
              <Input
                placeholder="Campaign name"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
              {/* Premise is optional; empty descriptions render no subtitle. */}
              <Textarea
                placeholder="Short premise (optional)"
                value={description}
                onChange={(event) => setDescription(event.target.value)}
              />
              {/* Error badge right above the actions, where the cause reads. */}
              {error ? <span className="badge badge-danger">{error}</span> : null}
              {/* Disabled while creating (double-click guard) or while the
                  name is blank — the latter is a visible hint, not just the
                  guard. */}
              <Button variant="primary" onClick={handleCreate} disabled={creating || name.trim().length === 0}>
                {/* Swap label for spinner so the user sees progress in-place. */}
                {creating ? <Spinner /> : "Create campaign"}
              </Button>
            </div>
          </Card>
        ) : null}

        {/* Empty state guides the user; no campaigns yet is the common first
            run, so it gets prose rather than a bare empty list. */}
        {sorted.length === 0 ? (
          <p className="muted">No roleplays yet — create your first above.</p>
        ) : (
          // Each row pairs a full-card open target with a delete action; the
          // row itself is a div (not a button) because Delete must sit inside.
          sorted.map((campaign) => (
            <div key={campaign.id} className="campaign-row">
              {/* The open target stretches across the row; clicking anywhere
                  on the name/description block enters the campaign's chat. */}
              <button className="campaign-row-open" onClick={() => onOpenCampaign(campaign.id)}>
                <div className="col" style={{ gap: 2 }}>
                  <span className="campaign-row-name">{campaign.name}</span>
                  {/* Subtitle only when a premise was written at creation. */}
                  {campaign.description ? <span className="muted">{campaign.description}</span> : null}
                  {/* Last activity: a faint timestamp signals freshness and
                      helps the user tell similar campaigns apart. */}
                  <span className="campaign-row-meta">
                    Last activity {new Date(campaign.updated_at).toLocaleString()}
                  </span>
                </div>
              </button>
              {/* Danger-styled so the destructive action is visually distinct
                  from the primary open action beside it. */}
              <Button variant="danger" onClick={() => handleDelete(campaign)}>
                Delete
              </Button>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
