// Search panel: full-text search over a campaign's transcript (FTS5).

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { Badge, Button, Card, Input, Spinner } from "../../core/ui/components"
import { searchMessages } from "./api"

export function SearchPanel({ campaignId }: { campaignId: string }) {
  const [query, setQuery] = useState("")
  const [submitted, setSubmitted] = useState("")

  const { data: results, isLoading } = useQuery({
    queryKey: ["search", campaignId, submitted],
    queryFn: () => searchMessages(campaignId, submitted, 50),
    enabled: submitted.trim().length > 0,
  })

  function handleSearch() {
    setSubmitted(query.trim())
  }

  return (
    <div className="col">
      <div className="row">
        <Input
          placeholder="Search the transcript..."
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              handleSearch()
            }
          }}
          className="grow"
        />
        <Button variant="primary" onClick={handleSearch} disabled={!query.trim()}>
          Search
        </Button>
      </div>

      {isLoading ? (
        <Spinner label="Searching..." />
      ) : !submitted ? (
        <p className="muted">Search finds matching lines in the campaign's transcript (FTS5).</p>
      ) : (
        <div className="col">
          {!results || results.length === 0 ? (
            <p className="muted">No matches for "{submitted}".</p>
          ) : (
            results.map((result) => (
              <Card key={result.message_id}>
                <div className="row">
                  <Badge tone={result.role === "assistant" ? "accent" : undefined}>
                    {result.role}
                  </Badge>
                  <span className="faint">turn {result.turn_index}</span>
                </div>
                <pre className="muted" style={{ whiteSpace: "pre-wrap", margin: 0 }}>
                  {result.snippet ??
                    result.content
                      .filter((block) => block.type === "text")
                      .map((block) => block.text)
                      .join(" ")}
                </pre>
              </Card>
            ))
          )}
        </div>
      )}
    </div>
  )
}
