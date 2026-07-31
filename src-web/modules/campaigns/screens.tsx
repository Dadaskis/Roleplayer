// Campaign list + creation screen (the app's landing view).

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { Button, Card, Input, Spinner, Textarea } from "../../core/ui/components"
import { errorMessage } from "../../core/api/invoke"
import { createCampaign, deleteCampaign, listCampaigns } from "./api"
import { useCampaignStore } from "./store"
import type { Campaign } from "./types"

export function CampaignsScreen({ onOpenCampaign }: { onOpenCampaign: (campaignId: string) => void }) {
  const queryClient = useQueryClient()
  const { activeCampaignId, setActiveCampaign } = useCampaignStore()
  const [name, setName] = useState("")
  const [description, setDescription] = useState("")
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const { data: campaigns, isLoading } = useQuery({
    queryKey: ["campaigns"],
    queryFn: listCampaigns,
  })

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["campaigns"] })

  async function handleCreate() {
    if (!name.trim()) {
      setError("A campaign needs a name.")
      return
    }
    setCreating(true)
    setError(null)
    try {
      const campaign = await createCampaign({ name, description, ruleset_id: null })
      setName("")
      setDescription("")
      refresh()
      onOpenCampaign(campaign.id)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setCreating(false)
    }
  }

  async function handleDelete(campaign: Campaign) {
    if (!confirm(`Delete "${campaign.name}" and all of its data? This cannot be undone.`)) {
      return
    }
    await deleteCampaign(campaign.id)
    if (activeCampaignId === campaign.id) {
      setActiveCampaign(null)
    }
    refresh()
  }

  if (isLoading) {
    return <Spinner label="Loading campaigns..." />
  }

  return (
    <div className="col">
      <h1 className="sidebar-title">Roleplayer</h1>

      <Card title="New roleplay">
        <div className="col">
          <Input
            placeholder="Campaign name"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <Textarea
            placeholder="Short premise (optional)"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          <Button variant="primary" onClick={handleCreate} disabled={creating || name.trim().length === 0}>
            {creating ? <Spinner /> : "Create campaign"}
          </Button>
        </div>
      </Card>

      <Card title={`Your campaigns (${campaigns?.length ?? 0})`}>
        {!campaigns || campaigns.length === 0 ? (
          <p className="muted">No campaigns yet — create your first roleplay above.</p>
        ) : (
          <div className="col">
            {campaigns.map((campaign) => (
              <div key={campaign.id} className="row card" style={{ justifyContent: "space-between" }}>
                <button
                  className="btn btn-ghost"
                  style={{ textAlign: "left" }}
                  onClick={() => onOpenCampaign(campaign.id)}
                >
                  <div className="col" style={{ gap: 2 }}>
                    <strong>{campaign.name}</strong>
                    {campaign.description ? <span className="muted">{campaign.description}</span> : null}
                  </div>
                </button>
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
