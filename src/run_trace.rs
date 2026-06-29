use serde::{Deserialize, Serialize};

use crate::ids::AgentName;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    #[default]
    Stop,
    MaxIterations,
    Transfer,
    Suspended,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTrace {
    pub steps: Vec<RunTraceStep>,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunTraceStep {
    ModelCall {
        agent_name: AgentName,
        event_count: usize,
        tool_count: usize,
    },
    ToolCall {
        tool_name: String,
        call_id: String,
    },
    AgentTransfer {
        from_agent: AgentName,
        to_agent: AgentName,
    },
}
