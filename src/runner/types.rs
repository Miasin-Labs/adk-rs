use serde_json::Value;

use crate::Event;
use crate::agent::Agent;
use crate::approval::{ApprovalError, PendingApproval};
use crate::guardrail::GuardrailError;
use crate::ids::AgentName;
use crate::invocation::InvocationError;
use crate::model::ModelError;
use crate::plugin::PluginError;
use crate::run_trace::{FinishReason, RunTrace, RunTraceStep};
use crate::session::SessionError;
use crate::structured_output::StructuredOutputError;
use crate::tool::ToolError;

pub(super) struct CycleOutcome<'a> {
    pub events: Vec<Event>,
    pub next_agent: Option<&'a Agent>,
    pub finish_reason: FinishReason,
    pub trace_steps: Vec<RunTraceStep>,
    pub pending_approval: Option<PendingApproval>,
}

impl<'a> CycleOutcome<'a> {
    pub fn done(
        events: Vec<Event>,
        finish_reason: FinishReason,
        trace_steps: Vec<RunTraceStep>,
    ) -> Self {
        Self {
            events,
            next_agent: None,
            finish_reason,
            trace_steps,
            pending_approval: None,
        }
    }

    pub fn suspended(
        events: Vec<Event>,
        trace_steps: Vec<RunTraceStep>,
        pending_approval: PendingApproval,
    ) -> Self {
        Self {
            events,
            next_agent: None,
            finish_reason: FinishReason::Suspended,
            trace_steps,
            pending_approval: Some(pending_approval),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub events: Vec<Event>,
    pub transfer_to_agent: Option<AgentName>,
    pub finish_reason: FinishReason,
    pub trace: RunTrace,
    pub structured_output: Option<Value>,
    pub pending_approval: Option<PendingApproval>,
    /// Post-turn metric evaluations, if any `MetricEvaluator`s were configured.
    pub metrics: Vec<crate::metric::MetricEvaluation>,
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Invocation(#[from] InvocationError),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error(transparent)]
    StructuredOutput(#[from] StructuredOutputError),
    #[error(transparent)]
    Guardrail(#[from] GuardrailError),
    #[error(transparent)]
    Approval(#[from] ApprovalError),
    #[error(transparent)]
    Telemetry(#[from] crate::telemetry::TelemetryError),
    #[error(transparent)]
    Memory(#[from] crate::memory::MemoryError),
    #[error(transparent)]
    Planner(#[from] crate::planner::PlannerError),
    #[error("unknown tool {0}")]
    UnknownTool(String),
    #[error("unknown agent {0:?}")]
    UnknownAgent(AgentName),
}
