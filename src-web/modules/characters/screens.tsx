// Character roster panel: list + create/edit/delete for a campaign.

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"

import { errorMessage } from "../../core/api/invoke"
import { Button, Card, Input, Textarea } from "../../core/ui/components"
import { createCharacter, deleteCharacter, listCharacters, updateCharacter } from "./api"
import type { Character } from "./types"

export function CharactersPanel({ campaignId }: { campaignId: string }) {
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const [bio, setBio] = useState("")
  const [isPlayer, setIsPlayer] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const { data: characters, isLoading } = useQuery({
    queryKey: ["characters", campaignId],
    queryFn: () => listCharacters(campaignId),
  })

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["characters", campaignId] })

  async function handleCreate() {
    if (!name.trim()) {
      return
    }
    setError(null)
    try {
      await createCharacter({ campaign_id: campaignId, name, bio, is_player: isPlayer, stats: {} })
      setName("")
      setBio("")
      setIsPlayer(false)
      refresh()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  async function handleDelete(character: Character) {
    await deleteCharacter(character.id)
    refresh()
  }

  if (isLoading) {
    return <p className="muted">Loading characters...</p>
  }

  return (
    <div className="col">
      <div className="card">
        <h3 className="card-title">Add character</h3>
        <div className="col">
          <label className="row">
            <input type="checkbox" checked={isPlayer} onChange={(event) => setIsPlayer(event.target.checked)} />
            <span>Player character (not NPC)</span>
          </label>
          <Input placeholder="Name" value={name} onChange={(event) => setName(event.target.value)} />
          <Textarea placeholder="Short bio" value={bio} onChange={(event) => setBio(event.target.value)} />
          {error ? <span className="badge badge-danger">{error}</span> : null}
          <Button variant="primary" onClick={handleCreate} disabled={!name.trim()}>
            Add character
          </Button>
        </div>
      </div>

      {characters?.map((character) => (
        <Card key={character.id}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div className="col" style={{ gap: 2 }}>
              <strong>
                {character.name} {character.is_player ? <span className="badge badge-accent">player</span> : null}
              </strong>
              {character.bio ? <span className="muted">{character.bio}</span> : null}
            </div>
            <div className="row">
              <Button
                variant="ghost"
                onClick={() =>
                  updateCharacter(character.id, {
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
