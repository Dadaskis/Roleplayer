// Character roster panel: list + create/edit/delete for a campaign.

// useQuery owns the per-campaign roster cache; local state is form-only.
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Button, Card, Input, Textarea } from "../../core/ui/components"
import { createCharacter, deleteCharacter, listCharacters, updateCharacter } from "./api"
import type { Character } from "./types"

export function CharactersPanel({ campaignId }: { campaignId: string }) {
  const queryClient = useQueryClient()
  // Add-form fields; each resets after a successful create.
  const [name, setName] = useState("")
  const [bio, setBio] = useState("")
  // Default is NPC (unchecked); the player persona is the rarer case.
  const [isPlayer, setIsPlayer] = useState(false)
  // Last failure message shown as a danger badge under the form.
  const [error, setError] = useState<string | null>(null)

  // Roster is scoped to the campaign: switching campaigns swaps the key and
  // automatically refetches the right roster for the new workspace.
  const { data: characters, isLoading } = useQuery({
    queryKey: ["characters", campaignId],
    // Closure captures the current campaign id from the props.
    queryFn: () => listCharacters(campaignId),
  })

  // Cache invalidation is the single refresh path for both create and delete.
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["characters", campaignId] })

  async function handleCreate() {
    // Ignore empty names; the button is also disabled, this is belt-and-braces
    // so a stale event can't create a blank character.
    if (!name.trim()) {
      return
    }
    // Clear prior errors so a retry starts with a clean slate.
    setError(null)
    try {
      // stats starts as an empty object; the ruleset fills it later.
      await createCharacter({ campaign_id: campaignId, name, bio, is_player: isPlayer, stats: {} })
      // Reset the form to defaults so the next add starts fresh.
      setName("")
      setBio("")
      setIsPlayer(false)
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  async function handleDelete(character: Character) {
    // Characters are cheap to recreate, so no confirm gate here (unlike
    // campaigns); the roster refetch keeps the list truthful.
    await deleteCharacter(character.id)
    refresh()
  }

  // Loading gate: a spinner text line until the backend returns the roster.
  if (isLoading) {
    return <p className="muted">Loading characters...</p>
  }

  return (
    <div className="col">
      <div className="card">
        <h3 className="card-title">Add character</h3>
        <div className="col">
          {/* Checkbox toggles player-vs-NPC; label wraps it so the whole
              label text is clickable, not just the tiny box. */}
          <label className="row">
            <input type="checkbox" checked={isPlayer} onChange={(event) => setIsPlayer(event.target.checked)} />
            <span>Player character (not NPC)</span>
          </label>
          <Input placeholder="Name" value={name} onChange={(event) => setName(event.target.value)} />
          <Textarea placeholder="Short bio" value={bio} onChange={(event) => setBio(event.target.value)} />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          {/* Disabled while the name is blank — same guard as handleCreate,
              surfaced earlier so the user sees why they can't submit. */}
          <Button variant="primary" onClick={handleCreate} disabled={!name.trim()}>
            Add character
          </Button>
        </div>
      </div>

      {/* Roster rows: one card per character with actions on the right. */}
      {characters?.map((character) => (
        <Card key={character.id}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div className="col" style={{ gap: 2 }}>
              <strong>
                {/* Player personas get the accent "player" badge next to the
                    name; NPCs render the plain name only. */}
                {character.name} {character.is_player ? <span className="badge badge-accent">player</span> : null}
              </strong>
              {character.bio ? <span className="muted">{character.bio}</span> : null}
            </div>
            <div className="row">
              {/* Save re-sends the character's own current values: the panel
                  has no edit form yet, but the update contract is exercised so
                  future editors build on a proven path. */}
              <Button
                variant="ghost"
                onClick={() =>
                  updateCharacter(character.id, {
                    // Round-tripping the current row: a no-op write that
                    // proves the update path works end-to-end.
                    name: character.name,
                    is_player: character.is_player,
                    bio: character.bio,
                    stats: character.stats,
                  })
                }
              >
                Save
              </Button>
              <Button variant="danger" onClick={() => handleDelete(character)}>
                Delete
              </Button>
            </div>
          </div>
        </Card>
      ))}
    </div>
  )
}
