mod context;
mod cycle;
mod plugins;
mod resume;
mod types;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::agent::Agent;
use crate::app::App;
use crate::approval::PendingApproval;
use crate::auth::CredentialService;
use crate::event::{Event, EventActions, EventAuthor, EventPart};
use crate::guardrail::{GuardrailPhase, enforce_guardrails};
use crate::ids::{AgentName, EventId, InvocationId, SessionId};
use crate::invocation::InvocationContext;
use crate::plugin::Plugin;
use crate::run_config::RunConfig;
use crate::run_trace::RunTrace;
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

        let mut current_agent = &self.agent;
        let finish_reason;
        let pending_approval;
        loop {
            let outcome = self
                .run_llm_cycle(
                    current_agent,
                    &mut context,
                    &mut session,
                    invocation_id.clone(),
                )
                .await?;
            emitted.extend(outcome.events);
            trace.steps.extend(outcome.trace_steps);
            match outcome.next_agent {
                Some(next_agent) => current_agent = next_agent,
                None => {
                    finish_reason = outcome.finish_reason;
                    pending_approval = outcome.pending_approval;
                    break;
                }
            }
        }

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
