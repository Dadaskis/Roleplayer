// Ruleset manager: view/edit the GM's "brain" (system prompt) presets.

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Button, Card, Input, Textarea } from "../../core/ui/components"
import { createRuleset, deleteRuleset, listRulesets, updateRuleset } from "./api"
import type { Ruleset } from "./types"

export function RulesetsScreen() {
  // Query client to evict the cached list after create/save/delete.
  const queryClient = useQueryClient()
  // New-ruleset form fields; cleared after a successful create.
  const [name, setName] = useState("")
  const [systemPrompt, setSystemPrompt] = useState("")
  // Last failure from create or update, shown as a red banner.
  const [error, setError] = useState<string | null>(null)
  // Id of the ruleset currently in edit mode; null = all cards are previews.
  const [editingId, setEditingId] = useState<string | null>(null)

  const { data: rulesets, isLoading } = useQuery({ queryKey: ["rulesets"], queryFn: listRulesets })
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["rulesets"] })

  // Build a new ruleset from the form and prepend it to the list.
  async function handleCreate() {
    setError(null)
    try {
      // A fresh ruleset gets an empty world_rules bag; the prompt carries the
      // behavior, and rules can be enriched later through the same editor.
      await createRuleset({ name, system_prompt: systemPrompt, world_rules: {} })
      // Reset the form so the same name can't be re-submitted accidentally.
      setName("")
      setSystemPrompt("")
      // Refetch so the new ruleset card appears in the list.
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  // Persist edits made inside the currently-expanded edit form.
  async function handleSave(ruleset: Ruleset) {
    // Full-shape update: the edit form mutates the in-memory ruleset object
    // directly (see the onChange handlers), so this just ships it back whole.
    await updateRuleset(ruleset.id, {
      name: ruleset.name,
      system_prompt: ruleset.system_prompt,
      world_rules: ruleset.world_rules,
    })
    // Exit edit mode so the card collapses back to its preview.
    setEditingId(null)
    // Refetch so the stored prompt and card title reflect the save.
    refresh()
  }

  // Full-page placeholder while the ruleset list loads once.
  if (isLoading) {
    return <p className="muted">Loading rulesets...</p>
  }

  return (
    <div className="col">
      <Card title="New ruleset">
        <div className="col">
          {/* Controlled form: keystrokes update the create state. */}
          <Input placeholder="Ruleset name" value={name} onChange={(event) => setName(event.target.value)} />
          <Textarea
            placeholder="System prompt — how the GM behaves"
            value={systemPrompt}
            onChange={(event) => setSystemPrompt(event.target.value)}
          />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          {/* Disabled until both fields carry non-whitespace content. */}
          <Button variant="primary" onClick={handleCreate} disabled={!name.trim() || !systemPrompt.trim()}>
            Create ruleset
          </Button>
        </div>
      </Card>

      {rulesets?.map((ruleset) => (
        // Built-in rulesets get a suffix so their protected status is visible.
        <Card key={ruleset.id} title={ruleset.is_builtin ? `${ruleset.name} (built-in)` : ruleset.name}>
          {editingId === ruleset.id ? (
            // ---- Edit mode: inline inputs bound directly to the object. ----
            <div className="col">
              {/* Edit mode mutates the already-fetched object in place rather
                  than keeping a second copy of the form — the data is local
                  to this screen, so it's safe and avoids sync bugs. */}
              <Input
                value={ruleset.name}
                onChange={(event) => {
                  ruleset.name = event.target.value
                }}
              />
              <Textarea
                value={ruleset.system_prompt}
                onChange={(event) => {
                  ruleset.system_prompt = event.target.value
                }}
              />
              {/* Save ships the whole (mutated) object to the backend. */}
              <Button variant="primary" onClick={() => handleSave(ruleset)}>
                Save
              </Button>
            </div>
          ) : (
            // ---- View mode: truncated prompt + action buttons. ----
            <>
              {/* Preview the prompt truncated to ~400 chars with an ellipsis
                  marker; full editing happens in the Edit form, not here. */}
              <pre className="muted" style={{ whiteSpace: "pre-wrap", margin: 0 }}>
                {ruleset.system_prompt.slice(0, 400)}
                {ruleset.system_prompt.length > 400 ? "..." : ""}
              </pre>
              <div className="row">
                {!ruleset.is_builtin ? (
                  // User-made rulesets can be edited and deleted.
                  <>
                    {/* Enter edit mode for this card only. */}
                    <Button variant="ghost" onClick={() => setEditingId(ruleset.id)}>
                      Edit
                    </Button>
                    {/* Fire-and-forget delete; refetch removes the card. */}
                    <Button variant="danger" onClick={() => deleteRuleset(ruleset.id).then(refresh)}>
                      Delete
                    </Button>
                  </>
                ) : (
                  // Built-ins are read-only: no buttons, just the note.
                  <span className="faint">Built-in rulesets are protected.</span>
                )}
              </div>
            </>
          )}
        </Card>
      ))}
    </div>
  )
}
