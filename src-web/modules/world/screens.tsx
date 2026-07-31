// World-state panel: view the document, add/remove keys, and inspect the
// audit trail (the anti-hallucination record of every world change).

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Badge, Button, Card, Input } from "../../core/ui/components"
import { getWorldState, listStateChanges, removeWorldKey, setWorldKey } from "./api"

// Pretty-print any world value for display. JSON.stringify can throw on
// circular structures (defensive: untrusted backend data), so fall back to
// the coercion rather than crashing a render.
function formatValue(value: unknown): string {
  try {
    // Indent two spaces so nested world objects read as a structured doc.
    return JSON.stringify(value, null, 2)
  } catch {
    // Circular or otherwise non-serializable value: show its string coercion
    // so the panel never crashes on a hostile backend shape.
    return String(value)
  }
}

// `campaignId` scopes the doc, the trail, and every mutation to one campaign;
// switching campaigns remounts this panel via the routed parent.
export function WorldPanel({ campaignId }: { campaignId: string }) {
  // Query client is needed to evict the two cached queries after a write.
  const queryClient = useQueryClient()
  // Form state for the "Add fact" inputs; cleared after a successful set.
  const [key, setKey] = useState("")
  const [value, setValue] = useState("")
  // Surfaces the last failed mutation as a dismissable red banner.
  const [error, setError] = useState<string | null>(null)

  // Two independent queries: the current document and the recent audit trail.
  // They use separate keys because they refetch at different rates and either
  // one may be invalidated without touching the other.
  const { data: document } = useQuery({
    queryKey: ["world", campaignId],
    queryFn: () => getWorldState(campaignId),
  })

  const { data: changes } = useQuery({
    queryKey: ["world-changes", campaignId],
    queryFn: () => listStateChanges(campaignId, 30),
  })

  // A manual edit changes both the doc and the trail, so both keys go stale.
  // Invalidate both so the next render refetches them in one go.
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["world", campaignId] })
    queryClient.invalidateQueries({ queryKey: ["world-changes", campaignId] })
  }

  // Persist a manually entered fact into the world document.
  async function handleSet() {
    // Require both a key and a value; the button is also disabled but the
    // guard keeps Enter-in-field submissions from hitting the backend.
    if (!key.trim() || !value.trim()) {
      return
    }
    // Clear any previous failure so a stale banner isn't shown after success.
    setError(null)
    try {
      // The world doc holds JSON, so structured input is preferable; but a
      // plain string is a valid value too. Try to parse — on failure keep the
      // raw string so partial input degrades instead of erroring.
      let parsed: unknown = value
      try {
        parsed = JSON.parse(value)
      } catch {
        // Keep the raw string when it is not valid JSON.
      }
      // Send the trimmed key and the parsed-or-raw value in one write; the
      // backend records the before/after pair in the audit trail.
      await setWorldKey(campaignId, key.trim(), parsed)
      // Reset the form so a repeat fact isn't accidentally re-submitted.
      setKey("")
      setValue("")
      // The write changed both the doc and the trail — refetch both.
      refresh()
    } catch (reason) {
      // Show the structured error message from the rejected IPC call.
      setError(errorMessage(reason))
    }
  }

  // Remove a key from the world document (audited like every other write).
  async function handleRemove(entryKey: string) {
    await removeWorldKey(campaignId, entryKey)
    // Both the doc (key gone) and the trail (new removal row) changed.
    refresh()
  }

  // The world doc is a free-form JSON object; guard the shape so a null or
  // non-object backend value yields an empty list instead of a crash.
  const entries: [string, unknown][] = document && typeof document === "object"
    // Only non-null objects have an entries() view; arrays would iterate by
    // index, which isn't a key/value doc, so cast and read as a record.
    ? Object.entries(document as Record<string, unknown>)
    : []

  return (
    <div className="col">
      <Card title="Add fact">
        <div className="row">
          {/* Controlled inputs: each keystroke updates the form state that
              `handleSet` reads; `grow` on the value gives it the wide slot. */}
          <Input placeholder="key (e.g. room_state)" value={key} onChange={(event) => setKey(event.target.value)} />
          <Input
            placeholder='value (JSON or text)'
            value={value}
            onChange={(event) => setValue(event.target.value)}
            className="grow"
          />
          {/* Disabled until both fields hold non-whitespace text. */}
          <Button variant="primary" onClick={handleSet} disabled={!key.trim() || !value.trim()}>
            Set
          </Button>
        </div>
        {/* Red banner with the last failure; cleared on the next attempt. */}
        {error ? <span className="badge badge-danger">{error}</span> : null}
      </Card>

      {/* The live world document: one row per key, with the raw JSON value. */}
      <Card title={`World state (${entries.length})`}>
        {entries.length === 0 ? (
          <p className="muted">The world is empty. The GM will fill it via tools — or add facts manually.</p>
        ) : (
          <div className="col">
            {/* Destructure each entry tuple into its key/value halves. */}
            {entries.map(([entryKey, entryValue]) => (
              // `entryKey` is the stable React key — a world key never repeats.
              <div key={entryKey} className="row" style={{ justifyContent: "space-between" }}>
                <div className="col" style={{ gap: 2 }}>
                  {/* Key shown as code, value pretty-printed below it. */}
                  <code>{entryKey}</code>
                  <pre className="muted" style={{ margin: 0, whiteSpace: "pre-wrap" }}>
                    {formatValue(entryValue)}
                  </pre>
                </div>
                {/* Destructive button per row; the backend audit-trails it. */}
                <Button variant="danger" onClick={() => handleRemove(entryKey)}>
                  Remove
                </Button>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* Recent mutations, newest first, capped at 30 by the query. */}
      <Card title="Audit trail (last changes)">
        {!changes || changes.length === 0 ? (
          <p className="muted">No world changes recorded yet.</p>
        ) : (
          <div className="col">
            {changes.map((change) => {
              // The tool that mutated the doc stamps its own args; the changed
              // key is conventionally `args.key`, but coerce defensively since
              // args are untyped backend data.
              const args = change.args as Record<string, unknown>
              const changeKey = String(args["key"] ?? "")
              return (
                <div key={change.id} className="col" style={{ gap: 2 }}>
                  {/* Header row: which tool, which key, when it happened. */}
                  <div className="row">
                    <Badge tone="accent">{change.tool}</Badge>
                    <code>{changeKey}</code>
                    {/* ISO timestamp shown in the user's local timezone. */}
                    <span className="faint">{new Date(change.created_at).toLocaleString()}</span>
                  </div>
                  {/* The before/after pair — anti-hallucination evidence. */}
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
