mod plugins;

use std::sync::Arc;

use crate::agent::Agent;
use crate::app::App;
use crate::event::{Event, EventActions, EventAuthor, EventPart};
use crate::ids::{AgentName, EventId, InvocationId, SessionId};
use crate::invocation::{InvocationContext, InvocationError};
use crate::model::{ModelError, ModelRequest};
use crate::plugin::{Plugin, PluginError};
use crate::run_config::RunConfig;
use crate::session::{Session, SessionError, SessionStore};
use crate::tool::{ToolError, ToolResult};

pub struct Runner<S: SessionStore> {
    store: S,
    agent: Agent,
    plugins: Vec<Arc<dyn Plugin>>,
    run_config: RunConfig,
}

impl<S: SessionStore> Runner<S> {
    pub fn new(store: S, agent: Agent) -> Self {
        Self {
            store,
            agent,
            plugins: Vec::new(),
            run_config: RunConfig::default(),
        }
    }

    pub fn from_app(store: S, app: App) -> Self {
        Self {
            store,
            agent: app.root_agent,
            plugins: app.plugins,
            run_config: RunConfig::default(),
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
        let user_event = self
            .emit_event(
                &context,
                &mut session,
                Event::text(invocation_id.clone(), EventAuthor::User, input),
            )
            .await?;
        let mut emitted = vec![user_event];

        let mut current_agent = &self.agent;
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
            match outcome.next_agent {
                Some(next_agent) => current_agent = next_agent,
                None => break,
            }
        }

        let transfer_to_agent = last_transfer(&emitted);
        self.store.save(session)?;
        self.after_run(&context).await?;
        Ok(RunOutput {
            events: emitted,
            transfer_to_agent,
        })
    }

    async fn run_llm_cycle<'a>(
        &'a self,
        agent: &'a Agent,
        context: &mut InvocationContext,
        session: &mut Session,
        invocation_id: InvocationId,
    ) -> Result<CycleOutcome<'a>, RunError> {
        let mut emitted = Vec::new();
        context.agent_name = agent.name.clone();

        loop {
            context.increment_llm_call_count()?;
            let request = ModelRequest {
                instruction: agent.instruction.clone(),
                events: session.events.clone(),
                tools: agent.tools.iter().map(|t| t.spec()).collect(),
            };
            let response = self
                .generate_model_response(context, agent, request)
                .await?;
            let had_tool_calls = !response.tool_calls.is_empty();

            for call in response.tool_calls {
                let result = self.call_tool(context, agent, &call).await?;
                let event = tool_event(invocation_id.clone(), result);
                let event = self.emit_event(context, session, event).await?;
                emitted.push(event);
            }

            let next_agent_name = response.actions.transfer_to_agent.clone();
            let had_text = if let Some(text) = response.text {
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
                return Ok(CycleOutcome {
                    events: emitted,
                    next_agent: Some(next_agent),
                });
            }

            if had_text {
                return Ok(CycleOutcome {
                    events: emitted,
                    next_agent: None,
                });
            }

            if !had_tool_calls {
                return Ok(CycleOutcome {
                    events: emitted,
                    next_agent: None,
                });
            }
        }
    }
}

struct CycleOutcome<'a> {
    events: Vec<Event>,
    next_agent: Option<&'a Agent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub events: Vec<Event>,
    pub transfer_to_agent: Option<crate::ids::AgentName>,
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
    #[error("unknown tool {0}")]
    UnknownTool(String),
    #[error("unknown agent {0:?}")]
    UnknownAgent(AgentName),
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

fn last_transfer(events: &[Event]) -> Option<crate::ids::AgentName> {
    events
        .iter()
        .rev()
        .find_map(|event| event.actions.transfer_to_agent.clone())
}
