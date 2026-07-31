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
  // Query client to invalidate the provider list after save/default/clear.
  const queryClient = useQueryClient()
  // Id of the provider whose card is expanded; null = none expanded.
  const [selectedId, setSelectedId] = useState<string | null>(null)
  // Model selected in the dropdown; seeded from the provider's stored model.
  const [model, setModel] = useState("")
  // Password field for a new key; cleared after a successful keyring write.
  const [apiKey, setApiKey] = useState("")
  // Diagnostic result line from the last "Test connection" run.
  const [testOutput, setTestOutput] = useState<string | null>(null)
  // Tracks an in-flight test so the button spins and can't double-fire.
  const [testing, setTesting] = useState(false)
  // Tracks an in-flight save so the button spins and can't double-fire.
  const [saving, setSaving] = useState(false)
  // Last failure, shown as a red banner inside the expanded card.
  const [error, setError] = useState<string | null>(null)

  // Load all configured providers once; the cards render from this list.
  const { data: providers, isLoading } = useQuery({ queryKey: ["providers"], queryFn: listProviders })
  // Derive the full selected provider object from the id so handlers can read
  // its fields (id, base_url, model) without re-looking it up.
  const selected = providers?.find((provider) => provider.id === selectedId) ?? null

  // Models only load once a provider is selected; `enabled` prevents a query
  // for the null selection, and the fallback resolves an empty list so the
  // hook still returns a stable value.
  const { data: models, isLoading: modelsLoading } = useQuery({
    queryKey: ["models", selectedId],
    queryFn: () => (selectedId ? listModels(selectedId) : Promise.resolve([])),
    enabled: Boolean(selectedId),
  })

  // Config/key/default changes all mutate provider rows — invalidate the list
  // so badges (`default`, `has_key`) and models re-render from fresh data.
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["providers"] })

  // Persist model/config and key in one click, respecting blank fields.
  async function handleSave() {
    // Nothing to save without a selected provider (can't happen via UI).
    if (!selected) {
      return
    }
    // Lock the Save button for the duration of the writes.
    setSaving(true)
    setError(null)
    try {
      // Two independent edits on one Save: only push a config change when the
      // model actually differs, and only touch the keyring when a key was
      // typed — blank fields mean "leave the stored value alone".
      if (model.trim() && model !== selected.model) {
        // Config write carries the existing base_url so it isn't clobbered.
        await setProviderConfig(selected.id, { model: model.trim(), base_url: selected.base_url })
      }
      if (apiKey.trim()) {
        // One-way keyring write; the backend never echoes the key back.
        await setProviderApiKey(selected.id, apiKey.trim())
        // The key is a one-way write (never echoed back), so clear the field
        // optimistically rather than waiting for a read that won't happen.
        setApiKey("")
      }
      // Refresh so `has_key`/model badges reflect the writes above.
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      // Restore the button even if one of the writes failed.
      setSaving(false)
    }
  }

  // Test output is diagnostic, not form state: both success text and failure
  // messages land in the same result box so the user sees one narrative.
  async function handleTest() {
    if (!selected) {
      return
    }
    // Spin the Test button and clear the previous run's output.
    setTesting(true)
    setTestOutput(null)
    try {
      const result = await testProvider(selected.id)
      setTestOutput(result)
    } catch (reason) {
      // A failed round-trip still lands in the box, as an error string.
      setTestOutput(errorMessage(reason))
    } finally {
      // Restore the button regardless of pass/fail.
      setTesting(false)
    }
  }

  // Full-screen spinner until the provider list has loaded once.
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
          // `active` decides whether this card expands its editor section.
          const active = selectedId === provider.id
          return (
            <Card key={provider.id} className={active ? undefined : ""}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                {/* The header button is the picker; the entire card
                    area is clickable for the compact picker feel. */}
                <button
                  className="btn btn-ghost"
                  onClick={() => {
                    // Selecting a provider seeds the model field with the
                    // stored model and clears stale test output from a
                    // previously selected provider.
                    setSelectedId(provider.id)
                    setModel(provider.model)
                    setTestOutput(null)
                  }}
                >
                  <div className="col" style={{ gap: 2, textAlign: "left" }}>
                    <strong>{provider.name}</strong>
                    {/* Show the currently stored model as a subtitle. */}
                    <span className="faint">{provider.model}</span>
                  </div>
                </button>
                <div className="row">
                  {/* A "default" badge only on the provider turnflow uses. */}
                  {provider.is_default ? <Badge tone="accent">default</Badge> : null}
                  {/* "Use" marks this provider as the app-wide default; the
                      list refetches so the badge moves to the right card. */}
                  <Button variant="ghost" onClick={() => setDefaultProvider(provider.id).then(refresh)}>
                    Use
                  </Button>
                </div>
              </div>

              {/* Editor section is hidden until this provider is selected. */}
              {active ? (
                <div className="col" style={{ marginTop: 12 }}>
                  <label className="faint">Model</label>
                  {modelsLoading ? (
                    <Spinner label="Loading models..." />
                  ) : (
                    // Fall back to a single option (the stored model) when the
                    // catalog is empty or hasn't loaded — the select needs at
                    // least one option to render meaningfully.
                    <Select value={model} onChange={(event) => setModel(event.target.value)}>
                      {(models?.length ? models : [{ id: provider.model, name: provider.model }]).map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.name}
                        </option>
                      ))}
                    </Select>
                  )}

                  {/* The checkmark tells the user a key is already stored
                      without the key ever leaving the keyring. */}
                  <label className="faint">API key {provider.has_key ? "(stored ✓)" : ""}</label>
                  <div className="row">
                    {/* Hides the key; cleared after a successful save. */}
                    <Input
                      type="password"
                      placeholder="Paste API key"
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      className="grow"
                    />
                    {/* Only enabled when a key exists in the ring. */}
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
                    {/* Disabled while saving or with no input at all. */}
                    <Button variant="primary" onClick={handleSave} disabled={saving || (!model.trim() && !apiKey.trim())}>
                      {saving ? <Spinner /> : "Save"}
                    </Button>
                    <Button onClick={handleTest} disabled={testing}>
                      {testing ? <Spinner /> : "Test connection"}
                    </Button>
                  </div>
                  {/* Diagnostic output appears below the buttons as a card. */}
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
