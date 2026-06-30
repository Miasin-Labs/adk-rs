use crate::approval::{ApprovalError, ResumeDecision};
use crate::ids::SessionId;
use crate::invocation::InvocationContext;
use crate::run_trace::{FinishReason, RunTrace};
use crate::runner::types::{RunError, RunOutput};
use crate::runner::{Runner, tool_event};
use crate::session::{Session, SessionStore};
use crate::tool::ToolResult;

impl<S: SessionStore> Runner<S> {
    pub async fn resume_tool_call(
        &self,
        session_id: &SessionId,
        tool_call_id: &str,
        decision: ResumeDecision,
    ) -> Result<RunOutput, RunError> {
        let pending = self
            .pending_approvals
            .lock()
            .map_err(|_| ApprovalError::PendingApprovalNotFound(tool_call_id.to_owned()))?
            .remove(tool_call_id)
            .ok_or_else(|| ApprovalError::PendingApprovalNotFound(tool_call_id.to_owned()))?;
        if &pending.session_id != session_id {
            return Err(ApprovalError::PendingApprovalNotFound(tool_call_id.to_owned()).into());
        }
        let mut session = self
            .store
            .load(session_id)?
            .unwrap_or_else(|| Session::new(session_id.clone()));
        let context = InvocationContext::new(
            &session,
            pending.invocation_id.clone(),
            self.agent.name.clone(),
        )
        .with_run_config(self.run_config.clone());
        let result = match decision {
            ResumeDecision::Approved => {
                let call = pending.tool_call();
                self.call_tool(&context, &session, &self.agent, &call)
                    .await?
            }
            ResumeDecision::Declined => ToolResult {
                call_id: pending.tool_call_id.clone(),
                content: serde_json::json!({
                    "approved": false,
                    "message": "Tool execution declined",
                }),
            },
        };
        let event = tool_event(pending.invocation_id.clone(), result);
        let event = self.emit_event(&context, &mut session, event).await?;
        self.store.save(session)?;
        let trace = RunTrace {
            finish_reason: FinishReason::Stop,
            ..RunTrace::default()
        };
        Ok(RunOutput {
            events: vec![event],
            transfer_to_agent: None,
            finish_reason: FinishReason::Stop,
            trace,
            structured_output: None,
            pending_approval: None,
            metrics: Vec::new(),
        })
    }
}
