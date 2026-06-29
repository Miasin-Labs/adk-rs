//! An adk-rs plugin that forwards a run's events to the MCP client as
//! `notifications/progress`, so `run_agent` streams what the agent is doing.

use std::sync::atomic::{AtomicU64, Ordering};

use adk_rs::{Event, EventAuthor, EventPart, InvocationContext, Plugin, PluginError};
use async_trait::async_trait;
use rmcp::Peer;
use rmcp::RoleServer;
use rmcp::model::{ProgressNotificationParam, ProgressToken};

pub struct ProgressPlugin {
    peer: Peer<RoleServer>,
    token: ProgressToken,
    step: AtomicU64,
}

impl ProgressPlugin {
    pub fn new(peer: Peer<RoleServer>, token: ProgressToken) -> Self {
        Self {
            peer,
            token,
            step: AtomicU64::new(0),
        }
    }

    async fn notify(&self, message: String) {
        let step = self.step.fetch_add(1, Ordering::Relaxed) + 1;
        let param =
            ProgressNotificationParam::new(self.token.clone(), step as f64).with_message(message);
        // Fire-and-forget; a failed notification must not fail the run.
        let _ = self.peer.notify_progress(param).await;
    }
}

#[async_trait]
impl Plugin for ProgressPlugin {
    fn name(&self) -> &str {
        "mcp-progress"
    }

    async fn on_event(
        &self,
        _context: &InvocationContext,
        event: Event,
    ) -> Result<Event, PluginError> {
        let message = match &event.author {
            EventAuthor::Agent(_) => event.parts.iter().find_map(|part| match part {
                EventPart::ToolCall(call) => Some(format!("calling tool: {}", call.name)),
                EventPart::Text(_) => Some("writing the answer".to_owned()),
                EventPart::ToolResult(_) => None,
            }),
            EventAuthor::Tool(_) => Some("tool returned a result".to_owned()),
            EventAuthor::User => None,
        };
        if let Some(message) = message {
            self.notify(message).await;
        }
        Ok(event)
    }
}
