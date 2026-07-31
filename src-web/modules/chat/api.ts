// Typed IPC calls for the chat (turnflow) module.

import { call, type MessageDto } from "../../core/api/invoke"

export function sendTurn(campaignId: string, text: string): Promise<number> {
  return call<number>("send_turn", { campaignId, text })
}

export function cancelTurn(campaignId: string): Promise<void> {
  return call<void>("cancel_turn", { campaignId })
}

export function listMessages(campaignId: string, limit: number): Promise<MessageDto[]> {
  return call<MessageDto[]>("list_messages", { campaignId, limit })
}
