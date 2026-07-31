// Ruleset manager: view/edit the GM's "brain" (system prompt) presets.

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Button, Card, Input, Textarea } from "../../core/ui/components"
import { createRuleset, deleteRuleset, listRulesets, updateRuleset } from "./api"
import type { Ruleset } from "./types"

export function RulesetsScreen() {
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const [systemPrompt, setSystemPrompt] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)

  const { data: rulesets, isLoading } = useQuery({ queryKey: ["rulesets"], queryFn: listRulesets })
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["rulesets"] })

  async function handleCreate() {
    setError(null)
    try {
      await createRuleset({ name, system_prompt: systemPrompt, world_rules: {} })
      setName("")
      setSystemPrompt("")
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  async function handleSave(ruleset: Ruleset) {
    await updateRuleset(ruleset.id, {
      name: ruleset.name,
      system_prompt: ruleset.system_prompt,
      world_rules: ruleset.world_rules,
    })
    setEditingId(null)
    refresh()
  }

  if (isLoading) {
    return <p className="muted">Loading rulesets...</p>
  }

  return (
    <div className="col">
      <Card title="New ruleset">
        <div className="col">
          <Input placeholder="Ruleset name" value={name} onChange={(event) => setName(event.target.value)} />
          <Textarea
            placeholder="System prompt — how the GM behaves"
            value={systemPrompt}
            onChange={(event) => setSystemPrompt(event.target.value)}
          />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          <Button variant="primary" onClick={handleCreate} disabled={!name.trim() || !systemPrompt.trim()}>
            Create ruleset
          </Button>
        </div>
      </Card>

      {rulesets?.map((ruleset) => (
        <Card key={ruleset.id} title={ruleset.is_builtin ? `${ruleset.name} (built-in)` : ruleset.name}>
          {editingId === ruleset.id ? (
            <div className="col">
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
              <Button variant="primary" onClick={() => handleSave(ruleset)}>
                Save
              </Button>
            </div>
          ) : (
            <>
              <pre className="muted" style={{ whiteSpace: "pre-wrap", margin: 0 }}>
                {ruleset.system_prompt.slice(0, 400)}
                {ruleset.system_prompt.length > 400 ? "..." : ""}
              </pre>
              <div className="row">
                {!ruleset.is_builtin ? (
                  <>
                    <Button variant="ghost" onClick={() => setEditingId(ruleset.id)}>
                      Edit
                    </Button>
                    <Button variant="danger" onClick={() => deleteRuleset(ruleset.id).then(refresh)}>
                      Delete
                    </Button>
                  </>
                ) : (
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
