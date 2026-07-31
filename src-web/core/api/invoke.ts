// The single typed IPC contract with the Rust backend (§5.18 of AGENTS.md).
//
// Every Tauri command the frontend can call is wrapped here with its exact
// payload/return types. The Rust side and this file must stay in sync — a
// signature change is a coordinated change on both sides.
//
// Wire shapes mirror the Rust structs (serde, snake_case fields).

import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

/** Structured error the backend returns for a failed command (§5.15). */
export interface ErrorDto {
  message: string
  retryable: boolean
  correlation_id: string
}

/** Normalize a rejected invoke into a readable message. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error
  }
  if (error && typeof error === "object" && "message" in error) {
    return String((error as ErrorDto).message)
  }
  return "Unknown error"
}

/** Invoke a command; rejects with the normalized message on failure. */
export function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args)
}

// ---------- shared domain shapes ----------

export type Role = "system" | "user" | "assistant" | "tool"

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "tool_call"; id: string; tool: string; arguments: unknown }
  | { type: "tool_result"; id: string; result: unknown }

export interface MessageDto {
  id: string
  campaign_id: string
  role: Role
  content: ContentBlock[]
  model: string | null
  turn_index: number
  created_at: string
}

// ---------- turn events (forwarded from the Rust event bus) ----------

export type TurnEvent =
  | { type: "turn_delta"; campaign_id: string; delta: string }
  | { type: "turn_message"; campaign_id: string; message: MessageDto }
  | { type: "turn_tool_call"; campaign_id: string; tool: string; arguments: unknown }
  | { type: "turn_tool_result"; campaign_id: string; tool: string; ok: boolean }
  | { type: "turn_complete"; campaign_id: string; turn_index: number }
  | { type: "turn_error"; campaign_id: string; message: string }

/** Subscribe to turn events; returns an unsubscribe function. */
export function onTurnEvent(handler: (event: TurnEvent) => void): Promise<UnlistenFn> {
  return listen<TurnEvent>("turn-event", (event) => {
    handler(event.payload)
  })
}
