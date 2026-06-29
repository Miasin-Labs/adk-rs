use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::SessionId;
use crate::tool::ToolCall;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingApproval {
    pub session_id: SessionId,
    pub invocation_id: crate::ids::InvocationId,
    pub tool_call_id: String,
    pub tool_name: String,
    pub message: String,
    pub args: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeDecision {
    Approved,
    Declined,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApprovalError {
    #[error("pending approval not found for tool call {0}")]
    PendingApprovalNotFound(String),
}

impl PendingApproval {
    pub fn from_call(
        session_id: SessionId,
        invocation_id: crate::ids::InvocationId,
        call: &ToolCall,
        message: String,
    ) -> Self {
        Self {
            session_id,
            invocation_id,
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            message,
            args: call.args.clone(),
        }
    }

    pub fn tool_call(&self) -> ToolCall {
        ToolCall {
            id: self.tool_call_id.clone(),
            name: self.tool_name.clone(),
            args: self.args.clone(),
        }
    }
}
