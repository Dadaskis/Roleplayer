// The single typed IPC contract with the Rust backend (§5.18 of AGENTS.md).
//
// Every Tauri command the frontend can call is wrapped here with its exact
// payload/return types. The Rust side and this file must stay in sync — a
// signature change is a coordinated change on both sides.
//
// Wire shapes mirror the Rust structs (serde, snake_case fields).

// `invoke` is the single bridge across the IPC boundary (frontend → Rust);
// all module api/ files funnel through `call` below, never tauri directly.
import { invoke } from "@tauri-apps/api/core"
// `listen` forwards the Rust event bus to the UI; UnlistenFn is the handle
// the subscription returns so screens can detach on unmount.
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

/** Structured error the backend returns for a failed command (§5.15). */
export interface ErrorDto {
  // Human-readable summary safe to show in a badge; the detail is logged
  // backend-side against this id, not shipped to the UI.
  message: string
  // True when a retry is likely to succeed (transient); the UI uses this to
  // decide whether to offer a "try again" affordance.
  retryable: boolean
  // Correlates a UI error with the backend log line for the same failure.
  correlation_id: string
}

/** Normalize a rejected invoke into a readable message. */
export function errorMessage(error: unknown): string {
  // The Tauri runtime rejects with a string (the command's message), a
  // structured ErrorDto (backend ErrorResponse), or something unexpected —
  // normalize all three to a line the UI can render in a badge.
  // Case 1: Tauri rejects with the raw string message as-is.
  if (typeof error === "string") {
    return error
  }
  // Case 2: a structured error object — extract only the user-facing message,
  // never the stack or correlation internals.
  if (error && typeof error === "object" && "message" in error) {
    return String((error as ErrorDto).message)
  }
  // Case 3: anything else (undefined, null, numeric codes) collapses to one
  // stable fallback so the UI never renders "[object Object]".
  return "Unknown error"
}

/** Invoke a command; rejects with the normalized message on failure. */
export function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  // Args are a flat record because the Rust side deserializes Tauri command
  // arguments directly into its parameter structs (serde, snake_case).
  // The generic T is the command's DTO; callers rely on the Rust contract for
  // the shape, so a type drift surfaces as a compile-time mismatch here.
  return invoke<T>(command, args)
}

// ---------- shared domain shapes ----------

// Speaker of a message; mirrors the Rust Role enum used by the turnflow.
// "assistant" is the GM; "tool" rows are engine-internal and rarely surfaced.
export type Role = "system" | "user" | "assistant" | "tool"

// How a player message should be read: dialogue (speech) or narration (action).
// Mirrors the Rust MessageMode enum; "action" is the default for older rows.
export type MessageMode = "action" | "speech"

// Message content is a tagged union mirroring the Rust ContentBlock enum
// (§5.5) — tool arguments/results are deliberately untyped here because each
// tool defines its own payload shape.
// Discriminated on `type` so the chat UI can switch per variant.
export type ContentBlock =
  // Plain prose the user/GM actually reads.
  | { type: "text"; text: string }
  // The GM requested a game command; arguments carry the tool's own schema.
  | { type: "tool_call"; id: string; tool: string; arguments: unknown }
  // A tool's outcome; `result` is opaque to the UI (rendered as a badge).
  | { type: "tool_result"; id: string; result: unknown }

/** A persisted message. snake_case keys match the Rust struct; turn_index
 *  orders messages within a campaign so the transcript replays in order. */
export interface MessageDto {
  id: string
  campaign_id: string
  role: Role
  content: ContentBlock[]
  // Player rows: "action" (narration) or "speech" (dialogue); GM/tool rows
  // always carry "action" and never read it.
  mode: MessageMode
  // Model that produced the message; null for user/system/tool rows.
  model: string | null
  // Monotonic per-campaign counter that orders the persisted transcript.
  turn_index: number
  // ISO-8601 timestamp from the backend, used only for display.
  created_at: string
}

// ---------- turn events (forwarded from the Rust event bus) ----------

// The Rust event bus streams turn progress as discrete event kinds so the UI
// can update incrementally: deltas build the live bubble, turn_message is the
// authoritative final message, error/complete bookend the turn.
export type TurnEvent =
  | { type: "turn_delta"; campaign_id: string; delta: string }
  | { type: "turn_message"; campaign_id: string; message: MessageDto }
  | { type: "turn_tool_call"; campaign_id: string; tool: string; arguments: unknown }
  | { type: "turn_tool_result"; campaign_id: string; tool: string; ok: boolean }
  | { type: "turn_complete"; campaign_id: string; turn_index: number }
  | { type: "turn_error"; campaign_id: string; message: string }

// The subscription resolves asynchronously (Tauri returns UnlistenFn in a
// promise), so callers must release it on unmount or the callback outlives
// the screen — see the cleanup dance in chat/screens.tsx.
/** Subscribe to turn events; returns an unsubscribe function. */
export function onTurnEvent(handler: (event: TurnEvent) => void): Promise<UnlistenFn> {
  // listen() wires this callback to the "turn-event" channel the Rust event
  // bus broadcasts on; we unwrap .payload so handlers only see the typed union.
  return listen<TurnEvent>("turn-event", (event) => {
    handler(event.payload)
  })
}
