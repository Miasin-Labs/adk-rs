mod context;
mod cycle;
mod plugins;
mod resume;
mod stream;
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
use crate::memory::MemoryService;
use crate::metric::{MetricEvaluation, MetricEvaluator};
use crate::planner::Planner;
use crate::plugin::Plugin;
use crate::run_config::RunConfig;
use crate::run_trace::{FinishReason, RunTrace};
pub use crate::runner::stream::{RunStream, RunStreamItem};
pub use crate::runner::types::{RunError, RunOutput};
use crate::session::{Session, SessionStore};
use crate::skills::SkillRegistry;
use crate::structured_output::parse_structured_output;
use crate::telemetry::{TelemetrySink, TelemetrySpan};
use crate::tool::{ToolCall, ToolResult};

pub struct Runner<S: SessionStore> {
    store: S,
    agent: Agent,
    plugins: Vec<Arc<dyn Plugin>>,
    run_config: RunConfig,
    credential_service: Option<Arc<dyn CredentialService>>,
    pending_approvals: Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    telemetry: Option<Arc<dyn TelemetrySink>>,
    memory: Option<Arc<dyn MemoryService>>,
    skills: Option<Arc<SkillRegistry>>,
    planner: Option<Arc<dyn Planner>>,
    metrics: Vec<Arc<dyn MetricEvaluator + Send + Sync>>,
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
            telemetry: None,
            memory: None,
            skills: None,
            planner: None,
            metrics: Vec::new(),
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
            telemetry: None,
            memory: None,
            skills: None,
            planner: None,
            metrics: Vec::new(),
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

    /// Record a telemetry span per model call (and one for the whole run).
    pub fn telemetry(mut self, sink: Arc<dyn TelemetrySink>) -> Self {
        self.telemetry = Some(sink);
        self
    }

    /// Search this memory service with the user input and inject retrieved
    /// entries into the model instruction before the loop (RAG retrieval).
    pub fn memory(mut self, service: Arc<dyn MemoryService>) -> Self {
        self.memory = Some(service);
        self
    }

    /// Inject the registered skills' prompts into the model instruction.
    pub fn skills(mut self, registry: Arc<SkillRegistry>) -> Self {
        self.skills = Some(registry);
        self
    }

    /// Build a plan from the user input before the loop and inject its steps
    /// into the model instruction.
    pub fn planner(mut self, planner: Arc<dyn Planner>) -> Self {
        self.planner = Some(planner);
        self
    }

    /// Evaluate the final output against this metric after the run; results are
    /// returned on `RunOutput.metrics`.
    pub fn metric(mut self, evaluator: Arc<dyn MetricEvaluator + Send + Sync>) -> Self {
        self.metrics.push(evaluator);
        self
    }

    pub async fn run(
        &self,
        session_id: &SessionId,
        invocation_id: InvocationId,
        input: impl Into<String>,
    ) -> Result<RunOutput, RunError> {
        self.run_inner(session_id.clone(), invocation_id, input.into(), None)
            .await
    }

    /// Shared run path for `run` and `stream`. When `event_sink` is set, every
    /// emitted event is also forwarded to it as the run produces it.
    async fn run_inner(
        &self,
        session_id: SessionId,
        invocation_id: InvocationId,
        input: String,
        event_sink: Option<tokio::sync::mpsc::UnboundedSender<Event>>,
    ) -> Result<RunOutput, RunError> {
        let mut session = self
            .store
            .load(&session_id)?
            .unwrap_or_else(|| Session::new(session_id.clone()));
        let mut context =
            InvocationContext::new(&session, invocation_id.clone(), self.agent.name.clone())
                .with_run_config(self.run_config.clone());
        context.event_sink = event_sink;
        self.before_run(&context).await?;
        let input = self.on_user_message(&context, input).await?;
        enforce_guardrails(&self.run_config.guardrails, GuardrailPhase::Input, &input)?;

        // Build the run-level instruction preamble from planner + retrieved
        // memory + skills, so every model call in this run sees it.
        context.instruction_preamble = self.build_preamble(&context, &input).await?;

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

        // Emit a telemetry span for the whole run, counting model calls.
        if let Some(sink) = &self.telemetry {
            let model_calls = trace
                .steps
                .iter()
                .filter(|step| matches!(step, crate::run_trace::RunTraceStep::ModelCall { .. }))
                .count();
            let span = TelemetrySpan {
                name: format!("run:{}", self.agent.name.as_str()),
                trace_id: invocation_id.as_str().to_owned(),
                token_usage: None,
            };
            sink.record_span(span)?;
            // One span per model call, so call count is observable downstream.
            for _ in 1..model_calls {
                sink.record_span(TelemetrySpan {
                    name: format!("model_call:{}", self.agent.name.as_str()),
                    trace_id: invocation_id.as_str().to_owned(),
                    token_usage: None,
                })?;
            }
        }

        // Post-turn metric evaluation against the final output text.
        let metrics = self.evaluate_metrics(&emitted);

        self.store.save(session)?;
        self.after_run(&context).await?;
        Ok(RunOutput {
            events: emitted,
            transfer_to_agent,
            finish_reason,
            trace,
            structured_output,
            pending_approval,
            metrics,
        })
    }

    /// Assemble the run-level instruction preamble from the planner, retrieved
    /// memory, and registered skills (each optional).
    async fn build_preamble(
        &self,
        context: &InvocationContext,
        input: &str,
    ) -> Result<String, RunError> {
        let mut sections: Vec<String> = Vec::new();

        if let Some(planner) = &self.planner {
            let plan = planner.build_plan(context, input).await?;
            if !plan.steps.is_empty() {
                let steps = plan
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(i, step)| format!("{}. {}", i + 1, step.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                sections.push(format!("Plan:\n{steps}"));
            }
        }

        if let Some(memory) = &self.memory {
            let hits = memory.search_memory(&context.app_name, &context.user_id, input)?;
            if !hits.is_empty() {
                let recalled = hits
                    .iter()
                    .map(|entry| format!("- {}", entry.text))
                    .collect::<Vec<_>>()
                    .join("\n");
                sections.push(format!("Relevant memory:\n{recalled}"));
            }
        }

        if let Some(skills) = &self.skills {
            let listed = skills.list();
            if !listed.is_empty() {
                let rendered = listed
                    .iter()
                    .map(|skill| format!("- {}: {}", skill.name, skill.prompt))
                    .collect::<Vec<_>>()
                    .join("\n");
                sections.push(format!("Available skills:\n{rendered}"));
            }
        }

        Ok(sections.join("\n\n"))
    }

    /// Evaluate the run's final output text against the configured metrics.
    fn evaluate_metrics(&self, emitted: &[Event]) -> Vec<MetricEvaluation> {
        if self.metrics.is_empty() {
            return Vec::new();
        }
        let actual = final_agent_text(emitted);
        let actual_tools = emitted
            .iter()
            .flat_map(|event| event.parts.iter())
            .filter_map(|part| match part {
                EventPart::ToolCall(call) => Some(call.name.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let input = crate::metric::MetricInput {
            expected: String::new(),
            actual,
            expected_tools: Vec::new(),
            actual_tools,
            forbidden_terms: Vec::new(),
            grounded_terms: Vec::new(),
        };
        self.metrics
            .iter()
            .map(|metric| metric.evaluate(&input))
            .collect()
    }
}

/// The final agent text event in a run, if any (reverse scan).
fn final_agent_text(events: &[Event]) -> String {
    events
        .iter()
        .rev()
        .find_map(|event| match (&event.author, event.parts.first()) {
            (EventAuthor::Agent(_), Some(EventPart::Text(text))) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
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
    /// - `Parallel`: fan each sub-agent out as a genuinely isolated branch over
    ///   a snapshot of the session taken at fan-out (so branches do not see one
    ///   another's events), run the branches concurrently, then merge their
    ///   events back into the shared session in declaration order.
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
                    self.run_parallel(agent, context, session, invocation_id, emitted, trace)
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

    /// Fan `parent`'s sub-agents out as isolated, concurrent branches.
    ///
    /// Each branch runs against an independent clone of the session and context
    /// snapshotted at fan-out, so no branch observes another branch's events.
    /// Branches run concurrently; their events are then merged back into the
    /// shared session in declaration order. If any branch suspends (tool
    /// approval), the whole node reports suspended.
    async fn run_parallel(
        &self,
        parent: &Agent,
        context: &mut InvocationContext,
        session: &mut Session,
        invocation_id: InvocationId,
        emitted: &mut Vec<Event>,
        trace: &mut RunTrace,
    ) -> Result<NodeOutcome, RunError> {
        // Snapshot of the session/context every branch sees. `clone()` here is
        // the isolation boundary: branches mutate their own copies only.
        let branch_futures = parent.sub_agents.iter().map(|child| {
            let mut branch_session = session.clone();
            let mut branch_context = context.clone();
            let invocation_id = invocation_id.clone();
            async move {
                let mut branch_emitted = Vec::new();
                let mut branch_trace = RunTrace::default();
                let outcome = self
                    .run_node(
                        child,
                        &mut branch_context,
                        &mut branch_session,
                        invocation_id,
                        &mut branch_emitted,
                        &mut branch_trace,
                    )
                    .await?;
                Ok::<_, RunError>((branch_emitted, branch_trace.steps, outcome))
            }
        });
        let results = futures::future::join_all(branch_futures).await;

        // Merge branch results back into the shared session in declaration
        // order. Branch events already passed through `on_event` inside the
        // branch, so append them directly (re-running plugins would double-fire).
        let mut node_outcome = NodeOutcome::done(FinishReason::Stop);
        for result in results {
            let (branch_emitted, branch_trace_steps, outcome) = result?;
            for event in branch_emitted {
                session.append(event.clone());
                emitted.push(event);
            }
            trace.steps.extend(branch_trace_steps);
            // The first suspended branch wins the reported outcome.
            if outcome.is_suspended() && !node_outcome.is_suspended() {
                node_outcome = outcome;
            }
        }
        Ok(node_outcome)
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
