// Typed IPC calls for the chat (turnflow) module.

// `call` is the only backend bridge; MessageDto is shared from core so the
// transcript type is the same everywhere it is rendered.
import { call, type MessageDto, type MessageMode } from "../../core/api/invoke"

// Returns the turn_index of the completed turn, so the caller can confirm the
// turn landed and later reconcile the transcript via listMessages.
export function sendTurn(campaignId: string, text: string, mode: MessageMode): Promise<number> {
  // The mode ("action" | "speech") rides alongside the text so the GM can tell
  // dialogue from narration when it builds its context.
  return call<number>("send_turn", { campaignId, text, mode })
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

// Ask the backend to kick the setup-intro turn (the GM opens the session
// itself). Returns whether a turn actually started — the backend's guard makes
// repeated calls (StrictMode double-mount) safe no-ops.
export function startSetupIntro(campaignId: string): Promise<boolean> {
  return call<boolean>("start_setup_intro", { campaignId })
}

// Start the roleplay: the GM generates the world + characters and opens the
// story. Completion arrives as turn events and the campaign's status change.
export function startRoleplay(campaignId: string): Promise<void> {
  return call<void>("start_roleplay", { campaignId })
}
