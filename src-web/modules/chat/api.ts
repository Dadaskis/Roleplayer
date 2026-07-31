// Typed IPC calls for the chat (turnflow) module.

// `call` is the only backend bridge; MessageDto is shared from core so the
// transcript type is the same everywhere it is rendered.
import { call, type MessageDto } from "../../core/api/invoke"

// Returns the turn_index of the completed turn, so the caller can confirm the
// turn landed and later reconcile the transcript via listMessages.
export function sendTurn(campaignId: string, text: string): Promise<number> {
  // The resolved number is the finished turn's index; the UI itself ignores
  // it (events drive rendering), but it proves the turn actually landed.
  return call<number>("send_turn", { campaignId, text })
}

// Aborts an in-flight turn on the backend; a turn_error/turn_complete event
// follows, which is what actually resets the UI's streaming state.
export function cancelTurn(campaignId: string): Promise<void> {
  // Void on success: the real signal is the follow-up event, so there is
  // nothing to return here beyond "the abort request was accepted".
  return call<void>("cancel_turn", { campaignId })
}

// `limit` caps the transcript window (the live screen loads the most recent
// 400) — the backend paginates, so full history stays available on demand.
export function listMessages(campaignId: string, limit: number): Promise<MessageDto[]> {
  // Returns newest-to-oldest ordering handled by the backend; the screen
  // seeds its store from this array on mount.
  return call<MessageDto[]>("list_messages", { campaignId, limit })
}
