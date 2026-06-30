use tokio::sync::mpsc::UnboundedSender;

use crate::event::Event;
use crate::ids::{AgentName, AppName, InvocationId, SessionId, UserId};
use crate::run_config::RunConfig;
use crate::session::Session;

#[derive(Clone)]
pub struct InvocationContext {
    pub app_name: AppName,
    pub user_id: UserId,
    pub session_id: SessionId,
    pub invocation_id: InvocationId,
    pub agent_name: AgentName,
    pub run_config: RunConfig,
    pub events: Vec<Event>,
    /// Run-level text prepended to each model request's instruction. Populated
    /// before the loop from the planner, retrieved memory, and skills.
    pub instruction_preamble: String,
    /// When set, every emitted event is also forwarded here so `Runner::stream`
    /// can surface events incrementally as the run produces them.
    pub(crate) event_sink: Option<UnboundedSender<Event>>,
    llm_calls: u32,
}

impl InvocationContext {
    pub fn new(session: &Session, invocation_id: InvocationId, agent_name: AgentName) -> Self {
        Self {
            app_name: session.app_name.clone(),
            user_id: session.user_id.clone(),
            session_id: session.id.clone(),
            invocation_id,
            agent_name,
            run_config: RunConfig::default(),
            events: session.events.clone(),
            instruction_preamble: String::new(),
            event_sink: None,
            llm_calls: 0,
        }
    }

    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    /// Prepend `instruction_preamble` to a base instruction, if any.
    pub(crate) fn apply_preamble(&self, instruction: &str) -> String {
        if self.instruction_preamble.is_empty() {
            instruction.to_owned()
        } else if instruction.is_empty() {
            self.instruction_preamble.clone()
        } else {
            format!("{}\n\n{}", self.instruction_preamble, instruction)
        }
    }

    pub fn increment_llm_call_count(&mut self) -> Result<(), InvocationError> {
        self.llm_calls += 1;
        match self.run_config.max_llm_calls {
            Some(limit) if self.llm_calls > limit => Err(InvocationError::LlmCallsLimitExceeded {
                limit,
                attempted: self.llm_calls,
            }),
            _ => Ok(()),
        }
    }
}

impl std::fmt::Debug for InvocationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvocationContext")
            .field("app_name", &self.app_name)
            .field("user_id", &self.user_id)
            .field("session_id", &self.session_id)
            .field("invocation_id", &self.invocation_id)
            .field("agent_name", &self.agent_name)
            .field("events", &self.events.len())
            .field("instruction_preamble", &self.instruction_preamble)
            .field("streaming", &self.event_sink.is_some())
            .field("llm_calls", &self.llm_calls)
            .finish()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvocationError {
    #[error("LLM calls limit exceeded: limit {limit}, attempted {attempted}")]
    LlmCallsLimitExceeded { limit: u32, attempted: u32 },
}
