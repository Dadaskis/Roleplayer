//! A typed in-process event bus (§4.2 of PLAN.md).
//!
//! Modules publish events; the UI (via the app crate) subscribes and forwards
//! them over Tauri. This keeps modules decoupled from the UI and from each
//! other — adding a new subscriber is adding code, not special-casing a flow.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

/// Buffer of the broadcast channel. Tauri events are low-volume (per turn,
/// not per token batch), so a few thousand entries is generous headroom.
const EVENT_BUFFER: usize = 2048;

/// Events the app can publish. Tagged on the wire so the frontend can match.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    /// A text fragment of the streaming GM reply for `campaign_id`.
    TurnDelta { campaign_id: String, delta: String },
    /// A whole message became final (after streaming finished).
    TurnMessage { campaign_id: String, message: Value },
    /// The GM invoked a tool; the UI can show what it tried.
    TurnToolCall { campaign_id: String, tool: String, arguments: Value },
    /// The tool call resolved (ok or not) and was applied / rejected.
    TurnToolResult { campaign_id: String, tool: String, ok: bool },
    /// The full agentic turn finished (all tool loops resolved).
    TurnComplete { campaign_id: String, turn_index: i64 },
    /// A turn failed; the UI shows the summary, logs hold the detail.
    TurnError { campaign_id: String, message: String },
}

/// The event bus. Cheap to clone (the sender is shared).
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AppEvent>,
}

impl EventBus {
    /// Create a new bus with the standard buffer.
    pub fn new() -> EventBus {
        let (sender, _receiver) = broadcast::channel(EVENT_BUFFER);
        EventBus { sender }
    }

    /// Publish an event to all current subscribers.
    ///
    /// A missing subscriber (send error) is intentionally ignored — the bus is
    /// a fire-and-forget notification channel, not a delivery guarantee.
    pub fn publish(&self, event: AppEvent) {
        // Drop the result: no subscriber is a valid, quiet state.
        let _ = self.sender.send(event);
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        EventBus::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_delivers_to_subscribers() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();

        bus.publish(AppEvent::TurnComplete {
            campaign_id: "c1".to_string(),
            turn_index: 3,
        });

        let event = receiver.try_recv().expect("event should be delivered");
        match event {
            AppEvent::TurnComplete { campaign_id, turn_index } => {
                assert_eq!(campaign_id, "c1");
                assert_eq!(turn_index, 3);
            }
            other => panic!("wrong event kind: {other:?}"),
        }
    }

    #[test]
    fn bus_ignores_missing_subscribers() {
        // Publishing with nobody listening must not error or panic.
        let bus = EventBus::new();
        bus.publish(AppEvent::TurnComplete {
            campaign_id: "c1".to_string(),
            turn_index: 1,
        });
    }
}
