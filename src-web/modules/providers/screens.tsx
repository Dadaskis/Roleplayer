// Provider settings screen with an OpenCode-style picker (see PLAN.md §3):
// pick a provider from a list, then a model from its catalog, set the key, and
// test the connection. Bloom accent on the active/default provider.

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Badge, Button, Card, Input, Select, Spinner } from "../../core/ui/components"
import {
  clearProviderApiKey,
  listModels,
  listProviders,
  setDefaultProvider,
  setProviderApiKey,
  setProviderConfig,
  testProvider,
} from "./api"

export function ProvidersScreen() {
  const queryClient = useQueryClient()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [model, setModel] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [testOutput, setTestOutput] = useState<string | null>(null)
  const [testing, setTesting] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const { data: providers, isLoading } = useQuery({ queryKey: ["providers"], queryFn: listProviders })
  const selected = providers?.find((provider) => provider.id === selectedId) ?? null

  const { data: models, isLoading: modelsLoading } = useQuery({
    queryKey: ["models", selectedId],
    queryFn: () => (selectedId ? listModels(selectedId) : Promise.resolve([])),
    enabled: Boolean(selectedId),
  })

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["providers"] })

  async function handleSave() {
    if (!selected) {
      return
    }
    setSaving(true)
    setError(null)
    try {
      if (model.trim() && model !== selected.model) {
        await setProviderConfig(selected.id, { model: model.trim(), base_url: selected.base_url })
      }
      if (apiKey.trim()) {
        await setProviderApiKey(selected.id, apiKey.trim())
        setApiKey("")
      }
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }

  async function handleTest() {
    if (!selected) {
      return
    }
    setTesting(true)
    setTestOutput(null)
    try {
      const result = await testProvider(selected.id)
      setTestOutput(result)
    } catch (reason) {
      setTestOutput(errorMessage(reason))
    } finally {
      setTesting(false)
    }
  }

  if (isLoading) {
    return <Spinner label="Loading providers..." />
  }

  return (
    <div className="col">
      <h2 className="sidebar-title">Providers</h2>
      <p className="muted">
        Pick the provider and model the GM uses. Keys are stored in your OS keyring — never in the database.
      </p>

      <div className="col">
        {providers?.map((provider) => {
          const active = selectedId === provider.id
          return (
            <Card key={provider.id} className={active ? undefined : ""}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <button
                  className="btn btn-ghost"
                  onClick={() => {
                    setSelectedId(provider.id)
                    setModel(provider.model)
                    setTestOutput(null)
                  }}
                >
                  <div className="col" style={{ gap: 2, textAlign: "left" }}>
                    <strong>{provider.name}</strong>
                    <span className="faint">{provider.model}</span>
                  </div>
                </button>
                <div className="row">
                  {provider.is_default ? <Badge tone="accent">default</Badge> : null}
                  <Button variant="ghost" onClick={() => setDefaultProvider(provider.id).then(refresh)}>
                    Use
                  </Button>
                </div>
              </div>

              {active ? (
                <div className="col" style={{ marginTop: 12 }}>
                  <label className="faint">Model</label>
                  {modelsLoading ? (
                    <Spinner label="Loading models..." />
                  ) : (
                    <Select value={model} onChange={(event) => setModel(event.target.value)}>
                      {(models?.length ? models : [{ id: provider.model, name: provider.model }]).map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.name}
                        </option>
                      ))}
                    </Select>
                  )}

                  <label className="faint">API key {provider.has_key ? "(stored ✓)" : ""}</label>
                  <div className="row">
                    <Input
                      type="password"
                      placeholder="Paste API key"
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      className="grow"
                    />
                    <Button
                      variant="ghost"
                      onClick={() => clearProviderApiKey(provider.id).then(refresh)}
                      disabled={!provider.has_key}
                    >
                      Clear
                    </Button>
                  </div>

                  {error ? <span className="badge badge-danger">{error}</span> : null}

                  <div className="row">
                    <Button variant="primary" onClick={handleSave} disabled={saving || (!model.trim() && !apiKey.trim())}>
                      {saving ? <Spinner /> : "Save"}
                    </Button>
                    <Button onClick={handleTest} disabled={testing}>
                      {testing ? <Spinner /> : "Test connection"}
                    </Button>
                  </div>
                  {testOutput ? (
                    <pre className="card" style={{ margin: 0, whiteSpace: "pre-wrap" }}>
                      {testOutput}
                    </pre>
                  ) : null}
                </div>
              ) : null}
            </Card>
          )
        })}
      </div>
    </div>
  )
}
