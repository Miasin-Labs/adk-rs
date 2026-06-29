use crate::agent::Agent;
use crate::approval::{ApprovalError, PendingApproval};
use crate::event::{Event, EventAuthor};
use crate::guardrail::{GuardrailPhase, enforce_guardrails};
use crate::ids::InvocationId;
use crate::invocation::InvocationContext;
use crate::model::ModelRequest;
use crate::run_trace::{FinishReason, RunTraceStep};
use crate::runner::context::request_events;
use crate::runner::types::{CycleOutcome, RunError};
use crate::runner::{Runner, tool_event};
use crate::session::{Session, SessionStore};
use crate::tool::ToolApprovalPolicy;

impl<S: SessionStore> Runner<S> {
    pub(super) async fn run_llm_cycle<'a>(
        &'a self,
        agent: &'a Agent,
        context: &mut InvocationContext,
        session: &mut Session,
        invocation_id: InvocationId,
    ) -> Result<CycleOutcome<'a>, RunError> {
        let mut emitted = Vec::new();
        let mut trace_steps = Vec::new();
        context.agent_name = agent.name.clone();
        let max_iterations = self.run_config.max_iterations;
        let mut iterations = 0_u32;

        loop {
            if max_iterations.is_some_and(|limit| iterations >= limit) {
                return Ok(CycleOutcome::done(
                    emitted,
                    FinishReason::MaxIterations,
                    trace_steps,
                ));
            }
            iterations += 1;
            context.increment_llm_call_count()?;
            let request = ModelRequest {
                instruction: agent.instruction.clone(),
                events: request_events(&session.events, self.run_config.memory_window_events),
                tools: agent.tools.iter().map(|tool| tool.spec()).collect(),
            };
            trace_steps.push(RunTraceStep::ModelCall {
                agent_name: agent.name.clone(),
                event_count: request.events.len(),
                tool_count: request.tools.len(),
            });
            let response = self
                .generate_model_response(context, agent, request)
                .await?;
            let had_tool_calls = !response.tool_calls.is_empty();

            for call in response.tool_calls {
                enforce_guardrails(
                    &self.run_config.guardrails,
                    GuardrailPhase::ToolCall,
                    &call.name,
                )?;
                if let ToolApprovalPolicy::Required { message } =
                    self.approval_policy(agent, &call)?
                {
                    let pending = PendingApproval::from_call(
                        context.session_id.clone(),
                        invocation_id.clone(),
                        &call,
                        message,
                    );
                    self.pending_approvals
                        .lock()
                        .map_err(|_| {
                            RunError::Approval(ApprovalError::PendingApprovalNotFound(
                                call.id.clone(),
                            ))
                        })?
                        .insert(call.id.clone(), pending.clone());
                    return Ok(CycleOutcome::suspended(emitted, trace_steps, pending));
                }
                trace_steps.push(RunTraceStep::ToolCall {
                    tool_name: call.name.clone(),
                    call_id: call.id.clone(),
                });
                let result = self.call_tool(context, session, agent, &call).await?;
                let event = tool_event(invocation_id.clone(), result);
                let event = self.emit_event(context, session, event).await?;
                emitted.push(event);
            }

            let next_agent_name = response.actions.transfer_to_agent.clone();
            let had_text = if let Some(text) = response.text {
                enforce_guardrails(&self.run_config.guardrails, GuardrailPhase::Output, &text)?;
                let event = Event::text(
                    invocation_id.clone(),
                    EventAuthor::Agent(agent.name.clone()),
                    text,
                )
                .with_actions(response.actions);
                let event = self.emit_event(context, session, event).await?;
                emitted.push(event);
                true
            } else {
                false
            };

            if let Some(next_agent_name) = next_agent_name {
                let next_agent = self
                    .agent
                    .find_agent(&next_agent_name)
                    .ok_or_else(|| RunError::UnknownAgent(next_agent_name.clone()))?;
                trace_steps.push(RunTraceStep::AgentTransfer {
                    from_agent: agent.name.clone(),
                    to_agent: next_agent_name,
                });
                return Ok(CycleOutcome {
                    events: emitted,
                    next_agent: Some(next_agent),
                    finish_reason: FinishReason::Transfer,
                    trace_steps,
                    pending_approval: None,
                });
            }
            if had_text || !had_tool_calls {
                return Ok(CycleOutcome::done(emitted, FinishReason::Stop, trace_steps));
            }
        }
    }

    fn approval_policy(
        &self,
        agent: &Agent,
        call: &crate::tool::ToolCall,
    ) -> Result<ToolApprovalPolicy, RunError> {
        agent
            .tools
            .iter()
            .find(|tool| tool.spec().name == call.name)
            .map(|tool| tool.approval_policy())
            .ok_or_else(|| RunError::UnknownTool(call.name.clone()))
    }
}
