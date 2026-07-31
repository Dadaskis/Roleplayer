// Chat screen: streaming transcript + composer, driven by turn events.

import { useEffect, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

import { onTurnEvent, type ContentBlock, type MessageDto } from "../../core/api/invoke"
import { Button, Spinner } from "../../core/ui/components"
import { cancelTurn, listMessages, sendTurn } from "./api"
import { useChatStore } from "./store"

function roleLabel(role: MessageDto["role"]): string {
  switch (role) {
    case "user":
      return "You"
    case "assistant":
      return "GM"
    case "system":
      return "System"
    case "tool":
      return "Tool"
  }
}

function BlockView({ block }: { block: ContentBlock }) {
  switch (block.type) {
    case "text":
      return <>{block.text}</>
    case "tool_call":
      return (
        <span className="badge badge-accent">
          ⚙ {block.tool}
        </span>
      )
    case "tool_result":
      return <span className="badge">done</span>
  }
}

function MessageBubble({ message }: { message: MessageDto }) {
  const isUser = message.role === "user"
  const text = message.content.filter((block) => block.type === "text").map((block) => block.text).join("\n")
  const hasTools = message.content.some((block) => block.type === "tool_call" || block.type === "tool_result")

  return (
    <div className={`message-enter col`} style={{ alignItems: isUser ? "flex-end" : "flex-start", gap: 4 }}>
      <span className="faint">{roleLabel(message.role)}</span>
      <div
        className="card"
        style={{
          maxWidth: "80%",
          background: isUser ? "rgba(165,180,252,0.12)" : undefined,
          borderColor: isUser ? "rgba(165,180,252,0.3)" : undefined,
        }}
      >
        {text ? <div style={{ whiteSpace: "pre-wrap" }}>{text}</div> : null}
        {hasTools ? (
          <div className="row" style={{ marginTop: text ? 8 : 0, flexWrap: "wrap" }}>
            {message.content.map((block, index) => (
              <BlockView key={index} block={block} />
            ))}
          </div>
        ) : null}
      </div>
    </div>
  )
}

export function ChatScreen({ campaignId }: { campaignId: string }) {
  const queryClient = useQueryClient()
  const [input, setInput] = useState("")
  const scrollRef = useRef<HTMLDivElement>(null)

  const messages = useChatStore((state) => state.byCampaign[campaignId] ?? [])
  const draft = useChatStore((state) => state.drafts[campaignId] ?? "")
  const streaming = useChatStore((state) => state.streaming[campaignId] ?? false)
  const activity = useChatStore((state) => state.activity[campaignId] ?? "")
  const error = useChatStore((state) => state.errors[campaignId] ?? null)
  const store = useChatStore.getState()

  // Load the persisted transcript on first mount.
  const { data: loaded } = useQuery({
    queryKey: ["messages", campaignId],
    queryFn: () => listMessages(campaignId, 400),
  })
  useEffect(() => {
    if (loaded) {
      store.setInitial(campaignId, loaded)
    }
  }, [loaded]) // eslint-disable-line react-hooks/exhaustive-deps

  // Subscribe to the live turn-event stream for this campaign.
  useEffect(() => {
    let unsubscribe: (() => void) | null = null
    let active = true

    onTurnEvent((event) => {
      if (event.campaign_id !== campaignId) {
        return
      }
      switch (event.type) {
        case "turn_delta":
          store.appendDelta(campaignId, event.delta)
          store.setStreaming(campaignId, true)
          break
        case "turn_message":
          store.appendMessage(campaignId, event.message)
          store.resetDraft(campaignId)
          break
        case "turn_tool_call":
          store.setActivity(campaignId, `GM used tool: ${event.tool}`)
          break
        case "turn_tool_result":
          store.setActivity(campaignId, null)
          break
        case "turn_complete":
          store.setStreaming(campaignId, false)
          store.setActivity(campaignId, null)
          // Reconcile with the authoritative transcript.
          queryClient.invalidateQueries({ queryKey: ["messages", campaignId] })
          break
        case "turn_error":
          store.setStreaming(campaignId, false)
          store.setError(campaignId, event.message)
          break
      }
    }).then((unlisten) => {
      if (active) {
        unsubscribe = unlisten
      } else {
        unlisten()
      }
    })

    return () => {
      active = false
      unsubscribe?.()
    }
  }, [campaignId]) // eslint-disable-line react-hooks/exhaustive-deps

  // Keep the transcript scrolled to the newest message.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
  }, [messages, draft, streaming])

  async function handleSend() {
    const text = input.trim()
    if (!text || streaming) {
      return
    }
    setInput("")
    store.setStreaming(campaignId, true)
    store.setError(campaignId, null)
    try {
      await sendTurn(campaignId, text)
    } catch (reason) {
      store.setStreaming(campaignId, false)
      store.setError(campaignId, String(reason))
    }
  }

  return (
    <div className="col" style={{ height: "100%" }}>
      <div ref={scrollRef} className="col grow" style={{ overflowY: "auto", padding: "16px 8px" }}>
        {messages.length === 0 && !draft ? (
          <p className="muted" style={{ margin: "auto", textAlign: "center" }}>
            The world is quiet. Say something to begin the roleplay.
          </p>
        ) : (
          messages.map((message) => <MessageBubble key={message.id} message={message} />)
        )}

        {streaming && draft ? (
          <div className="col" style={{ alignItems: "flex-start", gap: 4 }}>
            <span className="faint">GM</span>
            <div className="card">
              <span className="blink-caret" style={{ whiteSpace: "pre-wrap" }}>
                {draft}
              </span>
            </div>
          </div>
        ) : null}

        {streaming && activity ? <p className="faint">… {activity}</p> : null}

        {error ? <span className="badge badge-danger">{error}</span> : null}
      </div>

      <div className="col" style={{ borderTop: "1px solid var(--border)", padding: 12, gap: 8 }}>
        {streaming ? (
          <div className="row">
            <Spinner label="The GM is writing..." />
            <Button variant="ghost" onClick={() => cancelTurn(campaignId)}>
              Stop
            </Button>
          </div>
        ) : null}
        <div className="row">
          <textarea
            className="textarea grow"
            rows={3}
            placeholder="Describe what your character does..."
            value={input}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault()
                handleSend()
              }
            }}
          />
          <Button variant="primary" onClick={handleSend} disabled={!input.trim() || streaming}>
            Send
          </Button>
        </div>
      </div>
    </div>
  )
}
