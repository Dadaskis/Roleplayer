// World-state panel: view the document, add/remove keys, and inspect the
// audit trail (the anti-hallucination record of every world change).

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Badge, Button, Card, Input } from "../../core/ui/components"
import { getWorldState, listStateChanges, removeWorldKey, setWorldKey } from "./api"

function formatValue(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

export function WorldPanel({ campaignId }: { campaignId: string }) {
  const queryClient = useQueryClient()
  const [key, setKey] = useState("")
  const [value, setValue] = useState("")
  const [error, setError] = useState<string | null>(null)

  const { data: document } = useQuery({
    queryKey: ["world", campaignId],
    queryFn: () => getWorldState(campaignId),
  })

  const { data: changes } = useQuery({
    queryKey: ["world-changes", campaignId],
    queryFn: () => listStateChanges(campaignId, 30),
  })

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["world", campaignId] })
    queryClient.invalidateQueries({ queryKey: ["world-changes", campaignId] })
  }

  async function handleSet() {
    if (!key.trim() || !value.trim()) {
      return
    }
    setError(null)
    try {
      let parsed: unknown = value
      try {
        parsed = JSON.parse(value)
      } catch {
        // Keep the raw string when it is not valid JSON.
      }
      await setWorldKey(campaignId, key.trim(), parsed)
      setKey("")
      setValue("")
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  async function handleRemove(entryKey: string) {
    await removeWorldKey(campaignId, entryKey)
    refresh()
  }

  const entries: [string, unknown][] = document && typeof document === "object"
    ? Object.entries(document as Record<string, unknown>)
    : []

  return (
    <div className="col">
      <Card title="Add fact">
        <div className="row">
          <Input placeholder="key (e.g. room_state)" value={key} onChange={(event) => setKey(event.target.value)} />
          <Input
            placeholder='value (JSON or text)'
            value={value}
            onChange={(event) => setValue(event.target.value)}
            className="grow"
          />
          <Button variant="primary" onClick={handleSet} disabled={!key.trim() || !value.trim()}>
            Set
          </Button>
        </div>
        {error ? <span className="badge badge-danger">{error}</span> : null}
      </Card>

      <Card title={`World state (${entries.length})`}>
        {entries.length === 0 ? (
          <p className="muted">The world is empty. The GM will fill it via tools — or add facts manually.</p>
        ) : (
          <div className="col">
            {entries.map(([entryKey, entryValue]) => (
              <div key={entryKey} className="row" style={{ justifyContent: "space-between" }}>
                <div className="col" style={{ gap: 2 }}>
                  <code>{entryKey}</code>
                  <pre className="muted" style={{ margin: 0, whiteSpace: "pre-wrap" }}>
                    {formatValue(entryValue)}
                  </pre>
                </div>
                <Button variant="danger" onClick={() => handleRemove(entryKey)}>
                  Remove
                </Button>
              </div>
            ))}
          </div>
        )}
      </Card>

      <Card title="Audit trail (last changes)">
        {!changes || changes.length === 0 ? (
          <p className="muted">No world changes recorded yet.</p>
        ) : (
          <div className="col">
            {changes.map((change) => {
              const args = change.args as Record<string, unknown>
              const changeKey = String(args["key"] ?? "")
              return (
                <div key={change.id} className="col" style={{ gap: 2 }}>
                  <div className="row">
                    <Badge tone="accent">{change.tool}</Badge>
                    <code>{changeKey}</code>
                    <span className="faint">{new Date(change.created_at).toLocaleString()}</span>
                  </div>
                  <div className="row">
                    <span className="faint">before:</span>
                    <code>{formatValue(change.before_value)}</code>
                    <span className="faint">after:</span>
                    <code>{formatValue(change.after_value)}</code>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </Card>
    </div>
  )
}
