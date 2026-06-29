use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{AgentName, ArtifactName, EventId, InvocationId, StateKey};
use crate::tool::{ToolCall, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventAuthor {
    User,
    Agent(AgentName),
    Tool(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPart {
    Text(String),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventActions {
    pub skip_summarization: Option<bool>,
    pub state_delta: BTreeMap<StateKey, Value>,
    pub artifact_delta: BTreeMap<ArtifactName, u32>,
    pub transfer_to_agent: Option<AgentName>,
    pub escalate: Option<bool>,
    pub requested_auth_configs: BTreeMap<String, Value>,
    pub requested_tool_confirmations: BTreeMap<String, Value>,
    pub compaction: Option<Value>,
    pub end_of_agent: Option<bool>,
    pub agent_state: Option<Value>,
    pub rewind_before_invocation_id: Option<InvocationId>,
    pub route: Option<Value>,
    pub render_ui_widgets: Option<Vec<Value>>,
    pub set_model_response: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub invocation_id: InvocationId,
    pub author: EventAuthor,
    pub parts: Vec<EventPart>,
    pub actions: EventActions,
    pub timestamp_seconds: u64,
}

impl Event {
    pub fn text(
        invocation_id: InvocationId,
        author: EventAuthor,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: EventId::for_index(0),
            invocation_id,
            author,
            parts: vec![EventPart::Text(content.into())],
            actions: EventActions::default(),
            timestamp_seconds: 0,
        }
    }

    pub fn with_id(mut self, id: EventId) -> Self {
        self.id = id;
        self
    }

    pub fn with_actions(mut self, actions: EventActions) -> Self {
        self.actions = actions;
        self
    }
}
