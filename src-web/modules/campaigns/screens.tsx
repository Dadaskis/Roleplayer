// Campaign list + creation screen (the app's landing view).

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

export function CampaignsScreen({ onOpenCampaign }: { onOpenCampaign: (campaignId: string) => void }) {
  // The cache handle lets this screen invalidate the list after mutations.
  const queryClient = useQueryClient()
  // Store read for "which campaign is open", used to clear it on delete.
  const { activeCampaignId, setActiveCampaign } = useCampaignStore()
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
      // Clear the form, then jump straight into the new campaign so the user
      // can start roleplaying instead of hunting it in the list.
      setName("")
      setDescription("")
      // Refetch the list so the new row appears under both this screen and
      // the store's campaignsVersion consumers on their next read.
      refresh()
      // Hand the new id up; App flips to the workspace view on this call.
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
    // If the deleted campaign was the open workspace, clear the store too so
    // the app doesn't point at a now-gone id.
    if (activeCampaignId === campaign.id) {
      setActiveCampaign(null)
    }
    // After a delete the list must refetch; the UI decides this is needed
    // unconditionally since the backend already told us a row was removed.
    refresh()
  }

  // First paint: the list has not arrived yet, show the spinner immediately.
  if (isLoading) {
    return <Spinner label="Loading campaigns..." />
  }

  return (
    <div className="col">
      {/* Inline title so the landing view stands alone from the sidebar. */}
      <h1 className="sidebar-title">Roleplayer</h1>

      <Card title="New roleplay">
        <div className="col">
          {/* Controlled input: every keystroke updates `name` so the submit
              guard and the disabled state both read the latest value. */}
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
          {/* Disabled while creating (double-click guard) or while the name
              is blank — the latter is a visible hint, not just the guard. */}
          <Button variant="primary" onClick={handleCreate} disabled={creating || name.trim().length === 0}>
            {/* Swap label for spinner so the user sees progress in-place. */}
            {creating ? <Spinner /> : "Create campaign"}
          </Button>
        </div>
      </Card>

      {/* Count reflects the latest fetch; ?? 0 keeps it sane before load. */}
      <Card title={`Your campaigns (${campaigns?.length ?? 0})`}>
        {/* Empty state guides the user; no campaigns yet is the common first
            run, so it gets prose rather than a bare empty list. */}
        {!campaigns || campaigns.length === 0 ? (
          <p className="muted">No campaigns yet — create your first roleplay above.</p>
        ) : (
          <div className="col">
            {/* Each row pairs an open button with a delete action. */}
            {campaigns.map((campaign) => (
              <div key={campaign.id} className="row card" style={{ justifyContent: "space-between" }}>
                {/* Whole-card area opens the campaign; clicking anywhere on
                    the name/description block navigates into the workspace. */}
                <button
                  className="btn btn-ghost"
                  style={{ textAlign: "left" }}
                  onClick={() => onOpenCampaign(campaign.id)}
                >
                  <div className="col" style={{ gap: 2 }}>
                    <strong>{campaign.name}</strong>
                    {/* Subtitle only when a premise was written at creation */}
                    {campaign.description ? <span className="muted">{campaign.description}</span> : null}
                  </div>
                </button>
                {/* Danger-styled so the destructive action is visually distinct
                    from the primary open action beside it. */}
                <Button variant="danger" onClick={() => handleDelete(campaign)}>
                  Delete
                </Button>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  )
}
