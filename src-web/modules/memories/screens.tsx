// Memories panel: GM-curated long-term facts + provider-generated summaries.

import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

import { errorMessage } from "../../core/api/invoke"
import { Badge, Button, Card, Spinner, Textarea } from "../../core/ui/components"
import { createMemory, deleteMemory, listMemories, summarizeMemory } from "./api"

// Scoped to one campaign; the routed parent remounts this panel when the
// campaign changes so the query key always matches the current campaign.
export function MemoriesPanel({ campaignId }: { campaignId: string }) {
  // Query client to evict the cached memory list after add/generate/delete.
  const queryClient = useQueryClient()
  // Controlled textarea text; sent as the memory summary and cleared on save.
  const [summary, setSummary] = useState("")
  // Guards the generate button: disables it and swaps in a spinner while the
  // provider summarization request is in flight.
  const [generating, setGenerating] = useState(false)
  // Last operation failure, shown as a red banner above the buttons.
  const [error, setError] = useState<string | null>(null)

  const { data: memories, isLoading } = useQuery({
    queryKey: ["memories", campaignId],
    queryFn: () => listMemories(campaignId),
  })

  // After any mutation the cached list is stale; invalidate to refetch.
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["memories", campaignId] })

  // Persist the textarea's text as a hand-written long-term fact.
  async function handleAdd() {
    // Ignore blank summaries (the button is also disabled — belt and braces).
    if (!summary.trim()) {
      return
    }
    // Clear the previous banner so a success isn't shown alongside old errors.
    setError(null)
    try {
      // Hand-written memories aren't anchored to turns, so they use the 0..0
      // sentinel the backend treats as "no turn range".
      await createMemory({ campaign_id: campaignId, summary, source_from: 0, source_to: 0 })
      // Reset the input so the saved text isn't accidentally re-submitted.
      setSummary("")
      // The new memory must appear in the list — refetch it.
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  // Ask the provider to condense the transcript into a new memory.
  async function handleGenerate() {
    // Flip the button into its loading state before the async request.
    setGenerating(true)
    setError(null)
    try {
      // Summarize the entire transcript so far (0..max); turnflow assigns real
      // turn ranges, so 0..0 means "as far back as available".
      await summarizeMemory({ campaign_id: campaignId, source_from: 0, source_to: 0 })
      // The generated memory is persisted; refetch to show it in the list.
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      // Always restore the button even if the request failed or was aborted.
      setGenerating(false)
    }
  }

  // Show a full-screen spinner until the first list fetch resolves.
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
          {/* Controlled textarea; every keystroke updates the `summary` state
              that both create and generate paths read. */}
          <Textarea
            placeholder="e.g. The party swore a blood oath to the innkeeper of Duskmoor."
            value={summary}
            onChange={(event) => setSummary(event.target.value)}
          />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          <div className="row">
            {/* Manual save — disabled while there's nothing to save. */}
            <Button variant="primary" onClick={handleAdd} disabled={!summary.trim()}>
              Save memory
            </Button>
            {/* Generate — swapped to a spinner while the provider works. */}
            <Button onClick={handleGenerate} disabled={generating}>
              {generating ? <Spinner /> : "Summarize transcript"}
            </Button>
          </div>
        </div>
      </div>

      {/* Stored facts, newest conceptual entries first. */}
      {!memories || memories.length === 0 ? (
        <p className="muted">No memories yet.</p>
      ) : (
        memories.map((memory) => (
          <Card key={memory.id}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              {/* Left side: the fact text plus its originating turn range. */}
              <div className="col" style={{ gap: 2 }}>
                <p style={{ margin: 0 }}>{memory.summary}</p>
                {/* The source range proves the memory's provenance; 0..0
                    marks a hand-written memory with no transcript source. */}
                <span className="faint">
                  turns {memory.source_from}–{memory.source_to}
                </span>
              </div>
              <div className="row">
                <Badge tone="accent">memory</Badge>
                {/* Fire-and-forget delete: refetch the list on success so the
                    removed card disappears without a manual reload. */}
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
