mod context;
mod cycle;
mod plugins;
mod resume;
mod types;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::agent::{Agent, AgentKind};
use crate::app::App;
use crate::approval::PendingApproval;
use crate::auth::CredentialService;
use crate::event::{Event, EventActions, EventAuthor, EventPart};
use crate::guardrail::{GuardrailPhase, enforce_guardrails};
use crate::ids::{AgentName, EventId, InvocationId, SessionId};
use crate::invocation::InvocationContext;
use crate::plugin::Plugin;
use crate::run_config::RunConfig;
use crate::run_trace::{FinishReason, RunTrace};
pub use crate::runner::types::{RunError, RunOutput};
use crate::session::{Session, SessionStore};
use crate::structured_output::parse_structured_output;
use crate::tool::{ToolCall, ToolResult};

pub struct Runner<S: SessionStore> {
    store: S,
    agent: Agent,
    plugins: Vec<Arc<dyn Plugin>>,
    run_config: RunConfig,
    credential_service: Option<Arc<dyn CredentialService>>,
    pending_approvals: Arc<Mutex<BTreeMap<String, PendingApproval>>>,
}

impl<S: SessionStore> Runner<S> {
    pub fn new(store: S, agent: Agent) -> Self {
        Self {
            store,
            agent,
            plugins: Vec::new(),
            run_config: RunConfig::default(),
            credential_service: None,
            pending_approvals: Arc::default(),
        }
    }

    pub fn from_app(store: S, app: App) -> Self {
        Self {
            store,
            agent: app.root_agent,
            plugins: app.plugins,
            run_config: RunConfig::default(),
            credential_service: None,
            pending_approvals: Arc::default(),
        }
    }

    pub fn plugin(mut self, plugin: Arc<dyn Plugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    pub fn credential_service(mut self, service: Arc<dyn CredentialService>) -> Self {
        self.credential_service = Some(service);
        self
    }

    pub async fn run(
        &self,
        session_id: &SessionId,
        invocation_id: InvocationId,
        input: impl Into<String>,
    ) -> Result<RunOutput, RunError> {
        let mut session = self
            .store
            .load(session_id)?
            .unwrap_or_else(|| Session::new(session_id.clone()));
        let mut context =
            InvocationContext::new(&session, invocation_id.clone(), self.agent.name.clone())
                .with_run_config(self.run_config.clone());
        self.before_run(&context).await?;
        let input = self.on_user_message(&context, input.into()).await?;
        enforce_guardrails(&self.run_config.guardrails, GuardrailPhase::Input, &input)?;
        let user_event = self
            .emit_event(
                &context,
                &mut session,
                Event::text(invocation_id.clone(), EventAuthor::User, input),
            )
            .await?;
        let mut emitted = vec![user_event];
        let mut trace = RunTrace::default();

        // Orchestrate the root agent according to its `AgentKind` (Llm/handoff,
        // Sequential, Parallel, Loop). Returns the run's overall finish reason
        // and any pending approval that suspended it.
        let node = self
            .run_node(
                &self.agent,
                &mut context,
                &mut session,
                invocation_id.clone(),
                &mut emitted,
                &mut trace,
            )
            .await?;
        let finish_reason = node.finish_reason;
        let pending_approval = node.pending_approval;

        let transfer_to_agent = last_transfer(&emitted);
        trace.finish_reason = finish_reason;
        let structured_output =
            parse_structured_output(&emitted, self.run_config.structured_output_schema.as_ref())?;
        self.store.save(session)?;
        self.after_run(&context).await?;
        Ok(RunOutput {
            events: emitted,
            transfer_to_agent,
            finish_reason,
            trace,
            structured_output,
            pending_approval,
        })
    }
}

fn tool_event(invocation_id: InvocationId, result: ToolResult) -> Event {
    Event {
        id: EventId::for_index(0),
        invocation_id,
        author: EventAuthor::Tool(result.call_id.clone()),
        parts: vec![EventPart::ToolResult(result)],
        actions: EventActions::default(),
        timestamp_seconds: 0,
    }
}

/// The assistant event that records the model's tool calls. It must be appended
/// to the session before the tool-result events so the next model request keeps
/// each `tool` message paired with its preceding `tool_calls`.
fn tool_call_event(invocation_id: InvocationId, agent_name: AgentName, calls: &[ToolCall]) -> Event {
    Event {
        id: EventId::for_index(0),
        invocation_id,
        author: EventAuthor::Agent(agent_name),
        parts: calls.iter().cloned().map(EventPart::ToolCall).collect(),
        actions: EventActions::default(),
        timestamp_seconds: 0,
    }
}

fn last_transfer(events: &[Event]) -> Option<crate::ids::AgentName> {
    events
        .iter()
        .rev()
        .find_map(|event| event.actions.transfer_to_agent.clone())
}

/// True if any of `events` carries an `escalate` action, the loop-stop signal.
fn has_escalation(events: &[Event]) -> bool {
    events
        .iter()
        .any(|event| event.actions.escalate == Some(true))
}

/// Result of orchestrating one agent node (and its sub-tree).
struct NodeOutcome {
    finish_reason: FinishReason,
    pending_approval: Option<PendingApproval>,
}

impl NodeOutcome {
    fn done(finish_reason: FinishReason) -> Self {
        Self {
            finish_reason,
            pending_approval: None,
        }
    }

    fn suspended(pending: PendingApproval) -> Self {
        Self {
            finish_reason: FinishReason::Suspended,
            pending_approval: Some(pending),
        }
    }

    fn is_suspended(&self) -> bool {
        self.pending_approval.is_some()
    }
}

impl<S: SessionStore> Runner<S> {
    /// Orchestrate one agent according to its `AgentKind`, appending every
    /// emitted event to `emitted` and every trace step to `trace`.
    ///
    /// - `Llm`: a single LLM cycle, following model-driven `transfer_to_agent`
    ///   handoffs across the agent tree.
    /// - `Sequential`: run each sub-agent in declaration order over the shared
    ///   session, so each stage sees the prior stages' output.
    /// - `Parallel`: run each sub-agent independently over the shared session
    ///   (sequentially executed here, but with no data dependency between
    ///   branches — each branch only fans results back in).
    /// - `Loop`: re-run the sub-agent pipeline until a child escalates
    ///   (`EventActions::escalate`) or `max_iterations` is reached.
    ///
    /// A node with a workflow kind but no sub-agents degrades to a single LLM
    /// cycle, so nothing is silently skipped.
    fn run_node<'a>(
        &'a self,
        agent: &'a Agent,
        context: &'a mut InvocationContext,
        session: &'a mut Session,
        invocation_id: InvocationId,
        emitted: &'a mut Vec<Event>,
        trace: &'a mut RunTrace,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<NodeOutcome, RunError>> + Send + 'a>>
    {
        Box::pin(async move {
            match &agent.kind {
                AgentKind::Sequential if !agent.sub_agents.is_empty() => {
                    self.run_children_once(agent, context, session, invocation_id, emitted, trace)
                        .await
                }
                AgentKind::Parallel if !agent.sub_agents.is_empty() => {
                    self.run_children_once(agent, context, session, invocation_id, emitted, trace)
                        .await
                }
                AgentKind::Loop { max_iterations } if !agent.sub_agents.is_empty() => {
                    let mut iteration = 0_u32;
                    loop {
                        if *max_iterations != 0 && iteration >= *max_iterations {
                            break Ok(NodeOutcome::done(FinishReason::MaxIterations));
                        }
                        iteration += 1;
                        let before = emitted.len();
                        let outcome = self
                            .run_children_once(
                                agent,
                                context,
                                session,
                                invocation_id.clone(),
                                emitted,
                                trace,
                            )
                            .await?;
                        if outcome.is_suspended() {
                            break Ok(outcome);
                        }
                        // Stop as soon as a child escalates in this iteration.
                        if has_escalation(&emitted[before..]) {
                            break Ok(NodeOutcome::done(FinishReason::Stop));
                        }
                    }
                }
                // Llm, or a workflow kind with no sub-agents: a single cycle.
                _ => {
                    self.run_agent_with_handoffs(
                        agent,
                        context,
                        session,
                        invocation_id,
                        emitted,
                        trace,
                    )
                    .await
                }
            }
        })
    }

    /// Run every sub-agent of `parent` once, in declaration order, recursing so
    /// a child can itself be a workflow agent. Short-circuits on suspension.
    async fn run_children_once(
        &self,
        parent: &Agent,
        context: &mut InvocationContext,
        session: &mut Session,
        invocation_id: InvocationId,
        emitted: &mut Vec<Event>,
        trace: &mut RunTrace,
    ) -> Result<NodeOutcome, RunError> {
        let mut last = NodeOutcome::done(FinishReason::Stop);
        for child in &parent.sub_agents {
            last = self
                .run_node(
                    child,
                    context,
                    session,
                    invocation_id.clone(),
                    emitted,
                    trace,
                )
                .await?;
            if last.is_suspended() {
                return Ok(last);
            }
        }
        Ok(last)
    }

    /// Run a single agent through one or more LLM cycles, following any
    /// model-driven `transfer_to_agent` handoffs across the agent tree.
    async fn run_agent_with_handoffs<'a>(
        &'a self,
        agent: &'a Agent,
        context: &mut InvocationContext,
        session: &mut Session,
        invocation_id: InvocationId,
        emitted: &mut Vec<Event>,
        trace: &mut RunTrace,
    ) -> Result<NodeOutcome, RunError> {
        let mut current_agent = agent;
        loop {
            let outcome = self
                .run_llm_cycle(current_agent, context, session, invocation_id.clone())
                .await?;
            emitted.extend(outcome.events);
            trace.steps.extend(outcome.trace_steps);
            match outcome.next_agent {
                Some(next_agent) => current_agent = next_agent,
                None => {
                    return Ok(match outcome.pending_approval {
                        Some(pending) => NodeOutcome::suspended(pending),
                        None => NodeOutcome::done(outcome.finish_reason),
                    });
                }
            }
        }
    }
}
