// Memories panel: GM-curated long-term facts + provider-generated summaries.

import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

import { errorMessage } from "../../core/api/invoke"
import { Badge, Button, Card, Spinner, Textarea } from "../../core/ui/components"
import { createMemory, deleteMemory, listMemories, summarizeMemory } from "./api"

export function MemoriesPanel({ campaignId }: { campaignId: string }) {
  const queryClient = useQueryClient()
  const [summary, setSummary] = useState("")
  const [generating, setGenerating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const { data: memories, isLoading } = useQuery({
    queryKey: ["memories", campaignId],
    queryFn: () => listMemories(campaignId),
  })

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["memories", campaignId] })

  async function handleAdd() {
    if (!summary.trim()) {
      return
    }
    setError(null)
    try {
      await createMemory({ campaign_id: campaignId, summary, source_from: 0, source_to: 0 })
      setSummary("")
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  async function handleGenerate() {
    setGenerating(true)
    setError(null)
    try {
      // Summarize the entire transcript so far (0..max); turnflow assigns real
      // turn ranges, so 0..0 means "as far back as available".
      await summarizeMemory({ campaign_id: campaignId, source_from: 0, source_to: 0 })
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setGenerating(false)
    }
  }

  if (isLoading) {
    return <Spinner label="Loading memories..." />
  }

  return (
    <div className="col">
      <div className="card">
        <h3 className="card-title">Long-term memory</h3>
        <p className="muted">
          Facts that should survive beyond the context window. Add them manually, or generate a summary of the
          transcript with the current provider.
        </p>
        <div className="col">
          <Textarea
            placeholder="e.g. The party swore a blood oath to the innkeeper of Duskmoor."
            value={summary}
            onChange={(event) => setSummary(event.target.value)}
          />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          <div className="row">
            <Button variant="primary" onClick={handleAdd} disabled={!summary.trim()}>
              Save memory
            </Button>
            <Button onClick={handleGenerate} disabled={generating}>
              {generating ? <Spinner /> : "Summarize transcript"}
            </Button>
          </div>
        </div>
      </div>

      {!memories || memories.length === 0 ? (
        <p className="muted">No memories yet.</p>
      ) : (
        memories.map((memory) => (
          <Card key={memory.id}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <div className="col" style={{ gap: 2 }}>
                <p style={{ margin: 0 }}>{memory.summary}</p>
                <span className="faint">
                  turns {memory.source_from}–{memory.source_to}
                </span>
              </div>
              <div className="row">
                <Badge tone="accent">memory</Badge>
                <Button variant="danger" onClick={() => deleteMemory(memory.id).then(refresh)}>
                  Delete
                </Button>
              </div>
            </div>
          </Card>
        ))
      )}
    </div>
  )
}
