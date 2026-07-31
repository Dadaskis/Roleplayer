// Search panel: full-text search over a campaign's transcript (FTS5).

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { Badge, Button, Card, Input, Spinner } from "../../core/ui/components"
import { searchMessages } from "./api"

export function SearchPanel({ campaignId }: { campaignId: string }) {
  // Two states: `query` is the live input, `submitted` is what actually ran.
  // FTS is a backend round-trip, so search fires on Enter/click, not per
  // keystroke; keying the query off `submitted` makes that explicit.
  const [query, setQuery] = useState("")
  const [submitted, setSubmitted] = useState("")

  const { data: results, isLoading } = useQuery({
    // Keying on `submitted` (not `query`) is what makes Enter/click the only
    // trigger: typing updates `query` but the key stays put until a search.
    queryKey: ["search", campaignId, submitted],
    queryFn: () => searchMessages(campaignId, submitted, 50),
    // No query = no request; an empty key would otherwise fire a useless call.
    enabled: submitted.trim().length > 0,
  })

  // Commit the current input as the query that actually runs.
  function handleSearch() {
    // Trimming means whitespace-only input doesn't count as a search.
    setSubmitted(query.trim())
  }

  return (
    <div className="col">
      <div className="row">
        {/* Controlled input; Enter/Search commit a search. */}
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
        {/* Disabled while the live input is empty or whitespace-only. */}
        <Button variant="primary" onClick={handleSearch} disabled={!query.trim()}>
          Search
        </Button>
      </div>

      {/* Three states: in-flight, nothing submitted yet, or results. */}
      {isLoading ? (
        <Spinner label="Searching..." />
      ) : !submitted ? (
        // Idle hint before the first search.
        <p className="muted">Search finds matching lines in the campaign's transcript (FTS5).</p>
      ) : (
        <div className="col">
          {!results || results.length === 0 ? (
            // Submitted but the FTS5 pass came back empty.
            <p className="muted">No matches for "{submitted}".</p>
          ) : (
            // One card per ranked match, message_id as the stable key.
            results.map((result) => (
              <Card key={result.message_id}>
                <div className="row">
                  {/* Accent the assistant's lines; user lines stay neutral. */}
                  <Badge tone={result.role === "assistant" ? "accent" : undefined}>
                    {result.role}
                  </Badge>
                  <span className="faint">turn {result.turn_index}</span>
                </div>
                {/* Prefer the FTS snippet when the backend provided one;
                    otherwise rebuild a preview from the message's text blocks
                    so non-snippet backends still show something useful. */}
                <pre className="muted" style={{ whiteSpace: "pre-wrap", margin: 0 }}>
                  {result.snippet ??
                    result.content
                      // Only text blocks carry prose worth previewing.
                      .filter((block) => block.type === "text")
                      // Map each block to its plain text payload.
                      .map((block) => block.text)
                      // Concatenate blocks into one searchable line.
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
