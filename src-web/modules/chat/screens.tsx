// Chat screen: streaming transcript + composer, driven by turn events.
//
// Layout: a centered transcript column (720px measure) with ASYMMETRIC bubbles
// — the GM's narration is full-width prose with a left accent stripe (read
// like a book), the user's action is a contained right-aligned bubble. The
// composer dock below shares the same measure.

import { useEffect, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

import { onTurnEvent, type ContentBlock, type MessageDto } from "../../core/api/invoke"
import { Button } from "../../core/ui/components"
import { cancelTurn, listMessages, sendTurn } from "./api"
import { useChatStore } from "./store"

// Map wire roles to human-facing labels (the GM is the "assistant" role in
// the protocol; "tool" rows are engine-internal and get the plain name).
function roleLabel(role: MessageDto["role"]): string {
  // One label per protocol role; the switch is exhaustive over the union.
  switch (role) {
    case "user":
      return "You"
    case "assistant":
      // The GM speaks through the assistant role — this is the label the
      // player sees, never the engine name.
      return "GM"
    case "system":
      return "System"
    case "tool":
      // Tool rows are engine bookkeeping; the plain name keeps it honest.
      return "Tool"
  }
}

// Render one content block: tool calls/results surface as small mono chips
// rather than prose so the transcript shows *what the GM did* — and tool
// activity annotates the narration instead of interrupting it as a fake
// message.
function BlockView({ block }: { block: ContentBlock }) {
  // Discriminate on the `type` tag — each variant renders differently.
  switch (block.type) {
    case "text":
      // Prose renders verbatim; it is the bubble's main reading content.
      return <>{block.text}</>
    case "tool_call":
      // A called tool becomes a compact mono chip naming the tool; the
      // arguments stay hidden (raw JSON would bury the human narrative).
      return <span className="tool-chip">⚙ {block.tool}</span>
    case "tool_result":
      // Results collapse to a neutral "done" marker — outcome detail is not
      // something the player needs in the main transcript.
      return <span className="tool-chip">✓ done</span>
  }
}

function MessageBubble({ message }: { message: MessageDto }) {
  // Pull the prose blocks out for the main body and detect tool activity to
  // render the chip row below. Filter then map: concatenate the text blocks.
  const text = message.content.filter((block) => block.type === "text").map((block) => block.text).join("\n")
  // some() is a short-circuiting check — stops at the first tool block.
  const hasTools = message.content.some((block) => block.type === "tool_call" || block.type === "tool_result")

  // System lines are meta (session notes, rare) and get their own quiet row:
  // centered, tiny, no bubble, no label — clearly not part of the story.
  if (message.role === "system") {
    return <div className="msg-system">{text}</div>
  }

  // The user's own message is a short action statement: right-aligned bubble.
  const isUser = message.role === "user"
  return (
    // `message-enter` animates arrival; the msg-* class sets the asymmetry.
    <div className={`message-enter msg ${isUser ? "msg-user" : "msg-gm"}`}>
      {/* Role label above the bubble: who is speaking, in the faint type.
          roleLabel maps engine roles to human-facing names (GM/You). */}
      <span className="msg-label">{roleLabel(message.role)}</span>
      {/* pre-wrap preserves line breaks from the GM without a <br> pass. */}
      {text ? <div className="msg-body">{text}</div> : null}
      {/* Tool chips render only when the message carried tool blocks. */}
      {hasTools ? (
        // One chip per block, wrapped; blocks are immutable per message and
        // never reorder, so index keys cannot cause reconciliation churn.
        <div className="msg-tools">
          {message.content.map((block, index) => (
            <BlockView key={index} block={block} />
          ))}
        </div>
      ) : null}
    </div>
  )
}

export function ChatScreen({ campaignId }: { campaignId: string }) {
  // The cache handle is used to invalidate the transcript after turn_complete.
  const queryClient = useQueryClient()
  // Composer text; local state is fine — nothing else reads or writes it.
  const [input, setInput] = useState("")
  // The scroll container; used to jump to the newest message on updates.
  const scrollRef = useRef<HTMLDivElement>(null)

  // Reactive store reads: each subscription re-renders only this component
  // when its slice changes; ?? guards keep every value a concrete default.
  const messages = useChatStore((state) => state.byCampaign[campaignId] ?? [])
  const draft = useChatStore((state) => state.drafts[campaignId] ?? "")
  const streaming = useChatStore((state) => state.streaming[campaignId] ?? false)
  const activity = useChatStore((state) => state.activity[campaignId] ?? "")
  const error = useChatStore((state) => state.errors[campaignId] ?? null)
  // The imperative store handle is stable and non-reactive: safe to call from
  // event handlers and effects without subscribing to any slice.
  const store = useChatStore.getState()

  // Load the persisted transcript on first mount.
  // The query key is scoped to the campaign so switching campaigns refetches
  // the right transcript; `store` below is the Zustand API (stable), not a
  // reactive selector.
  const { data: loaded } = useQuery({
    queryKey: ["messages", campaignId],
    // 400 = the most recent window the live screen wants; older history is
    // available through the same command with a larger limit.
    queryFn: () => listMessages(campaignId, 400),
  })
  // Seed the store once when the transcript arrives. `store` is deliberately
  // excluded from deps — it is the stable store handle, not a changing value.
  useEffect(() => {
    if (loaded) {
      store.setInitial(campaignId, loaded)
    }
    // Deps is just [loaded]: campaignId changes remount the whole screen, so
    // it cannot go stale here; `store` is stable by construction.
  }, [loaded]) // eslint-disable-line react-hooks/exhaustive-deps

  // Subscribe to the live turn-event stream for this campaign.
  //
  // Lifecycle: Tauri's listen() resolves the unsubscribe function async, so
  // there is a window where the effect could unmount before it resolves.
  // `active` guards that race: if unmounted first we discard the late
  // unsubscribe; otherwise it is stashed for the cleanup below to release.
  useEffect(() => {
    // The unsubscribe handle once listen() resolves; null until then.
    let unsubscribe: (() => void) | null = null
    // Race guard: flipped false by cleanup if the screen unmounts before the
    // async listen() resolves (see the .then below).
    let active = true

    onTurnEvent((event) => {
      // Events are broadcast to every screen; ignore ones for other campaigns
      // so switching campaigns doesn't cross-feed this transcript.
      // The broadcast channel means every open screen receives every event,
      // so the campaign filter is what keeps transcripts isolated.
      if (event.campaign_id !== campaignId) {
        return
      }
      // One switch handles all event kinds; each case mutates store state,
      // which in turn re-renders the subscribers above.
      switch (event.type) {
        case "turn_delta":
          // Streamed prose accumulates in the draft buffer, rendered as the
          // live GM block; mark streaming so the composer locks and UI shows
          // the typing indicator.
          store.appendDelta(campaignId, event.delta)
          store.setStreaming(campaignId, true)
          break
        case "turn_message":
          // The authoritative final message lands in the transcript and the
          // draft buffer is discarded — the bubble's text was provisional.
          store.appendMessage(campaignId, event.message)
          // Belt-and-braces draft wipe even though appendMessage already
          // clears it: covers the case where appendMessage dedup'd (the
          // message already existed) but a stale draft is still showing.
          store.resetDraft(campaignId)
          break
        case "turn_tool_call":
          // Status line announces the tool; stays until the result resolves.
          store.setActivity(campaignId, `GM used tool: ${event.tool}`)
          break
        case "turn_tool_result":
          // Tool finished: clear the transient activity line.
          store.setActivity(campaignId, null)
          break
        case "turn_complete":
          // Turn over: unlock the composer and clear transient status.
          store.setStreaming(campaignId, false)
          store.setActivity(campaignId, null)
          // Reconcile with the authoritative transcript. The store may hold
          // an incomplete view (e.g. deltas without a final message), so a
          // refetch makes the persisted record the source of truth again.
          queryClient.invalidateQueries({ queryKey: ["messages", campaignId] })
          break
        case "turn_error":
          // A failed turn stops streaming and surfaces the backend message;
          // the user can retry without reloading the screen.
          store.setStreaming(campaignId, false)
          store.setError(campaignId, event.message)
          break
      }
    }).then((unlisten) => {
      if (active) {
        // Normal case: still mounted, keep the handle for cleanup below.
        unsubscribe = unlisten
      } else {
        // Unmounted before the subscription resolved — release it now rather
        // than leak a callback whose screen is gone.
        unlisten()
      }
    })

    return () => {
      // Mark inactive first so a late-resolving listen() can't re-stash a
      // dead subscription, then release the live one if we have it.
      // Order matters: `active = false` must precede `unsubscribe?.()` in
      // time, but this closure runs synchronously while listen() may resolve
      // later — the flag is what disambiguates the two orderings.
      active = false
      unsubscribe?.()
    }
  }, [campaignId]) // eslint-disable-line react-hooks/exhaustive-deps

  // Keep the transcript scrolled to the newest message.
  // Every delta updates the draft, so scrolling on [messages, draft,
  // streaming] follows a stream in real time.
  useEffect(() => {
    // Jump to the bottom: scrollHeight is the total content height, so
    // scrollTo snaps the viewport to the last rendered message.
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
    // Deps: scrolling must re-run on any of the three states that change the
    // transcript — persisted messages, the streaming draft, and the typing
    // indicator that appears/disappears with the streaming flag.
  }, [messages, draft, streaming])

  async function handleSend() {
    const text = input.trim()
    // No empty sends, and no overlapping turns — the backend is single-turn
    // per campaign, so the composer must wait for streaming to end.
    // Both guards read the same state the button's disabled attr uses, so a
    // stale click can't slip through after the UI already locked.
    if (!text || streaming) {
      return
    }
    // Clear the box immediately so the sent text doesn't linger editable.
    setInput("")
    // Optimistically lock the composer; the user's own message is *not*
    // appended here — the persisted transcript (refetched on turn_complete)
    // is the source of truth, and the events drive everything else.
    store.setStreaming(campaignId, true)
    // Drop any prior error so a fresh send doesn't show a stale failure.
    store.setError(campaignId, null)
    try {
      await sendTurn(campaignId, text)
    } catch (reason) {
      // A rejected send (e.g. backend unavailable) must release the lock so
      // the composer doesn't stay frozen; the error shows in the transcript.
      // String(reason) here (not errorMessage) because the abort case is a
      // local promise rejection, not a backend ErrorDto.
      store.setStreaming(campaignId, false)
      store.setError(campaignId, String(reason))
    }
  }

  return (
    // The screen is a full-height column: transcript above, composer pinned
    // at the bottom with a hairline separating the two zones. flex:1 (not
    // height:100%) lets it grow as the stage's flex child under the top bar.
    <div className="col" style={{ flex: 1, minHeight: 0 }}>
      {/* The scrollable transcript area; `grow` makes it fill all space
          above the composer so the composer never floats mid-window. */}
      <div className="transcript" ref={scrollRef}>
        <div className="transcript-inner">
          {/* Empty state: only when there is neither transcript nor draft —
              a brand-new campaign starts with this quiet nudge to act. */}
          {messages.length === 0 && !draft ? (
            <div className="empty-transcript">
              <p style={{ margin: "0 0 6px" }}>
                <strong>The world is quiet.</strong>
              </p>
              <p style={{ margin: 0 }}>Type an action to begin the roleplay.</p>
            </div>
          ) : (
            // One bubble per persisted message; id is the stable
            // reconciliation key, so re-renders never lose scroll or animate
            // existing rows.
            messages.map((message) => <MessageBubble key={message.id} message={message} />)
          )}

          {/* Live streaming block: rendered only while a turn is in flight
              with buffered text — the draft replaces the empty-state nudge.
              Rendered with the same full-width GM stripe as settled narration
              so streaming reads as "the GM is still writing this". */}
          {streaming && draft ? (
            <div className="msg msg-gm">
              <span className="msg-label">GM</span>
              {/* The blinking caret marks this block as live/in-progress,
                  distinct from the settled message blocks above. */}
              <div className="msg-body blink-caret">{draft}</div>
            </div>
          ) : null}

          {/* Transient status line (tool activity) only during a live turn. */}
          {streaming && activity ? <p className="faint">… {activity}</p> : null}

          {/* The last error shows inside the transcript so it is contextual,
              not a toast that could be missed. */}
          {error ? <span className="badge badge-danger">{error}</span> : null}
        </div>
      </div>

      {/* Composer dock: a bordered strip that stays put while the transcript
          above scrolls freely; its content is centered to the 720px measure. */}
      <div className="composer-dock">
        <div className="composer">
          {/* During a turn the composer collapses to a quiet typing line with
              Stop — replacing the input keeps the user's focus on the live
              state without a heavy spinner. */}
          {streaming ? (
            <div className="typing-line">
              {/* The pulsing dot is the whole "writing" indicator. */}
              <span className="typing-dot" aria-hidden="true" />
              <span>{activity ? activity : "The GM is writing…"}</span>
              {/* Abort path: the backend fires a follow-up event that finally
                  resets streaming, so this button only needs to request it. */}
              <Button variant="ghost" onClick={() => cancelTurn(campaignId)}>
                Stop
              </Button>
            </div>
          ) : null}
          <div className="composer-row">
            {/* Controlled textarea; .grow stretches it to share the row with
                the send button instead of wrapping onto a new line. The
                placeholder sets the roleplay tone — this is an action prompt,
                not a messaging box. */}
            <textarea
              className="textarea grow"
              rows={2}
              placeholder="Describe your action…"
              value={input}
              onChange={(event) => setInput(event.target.value)}
              // Enter sends, Shift+Enter inserts a newline — the standard chat
              // affordance; preventDefault stops the textarea from swallowing
              // the keystroke as a newline.
              onKeyDown={(event) => {
                // Bare Enter (no Shift) is the send gesture; anything else
                // (Shift+Enter, arrows, IME composition) falls through to the
                // textarea's default behavior.
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault()
                  handleSend()
                }
              }}
            />
            {/* Disabled when empty or streaming — the same two guards as
                handleSend, surfaced as a visible affordance hint. */}
            <Button variant="primary" onClick={handleSend} disabled={!input.trim() || streaming}>
              Send
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}
