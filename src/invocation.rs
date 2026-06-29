use crate::event::Event;
use crate::ids::{AgentName, AppName, InvocationId, SessionId, UserId};
use crate::run_config::RunConfig;
use crate::session::Session;

#[derive(Debug, Clone)]
pub struct InvocationContext {
    pub app_name: AppName,
    pub user_id: UserId,
    pub session_id: SessionId,
    pub invocation_id: InvocationId,
    pub agent_name: AgentName,
    pub run_config: RunConfig,
    pub events: Vec<Event>,
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
            llm_calls: 0,
        }
    }

    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvocationError {
    #[error("LLM calls limit exceeded: limit {limit}, attempted {attempted}")]
    LlmCallsLimitExceeded { limit: u32, attempted: u32 },
}
