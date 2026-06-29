use std::sync::Arc;

use crate::ids::AgentName;
use crate::model::LanguageModel;
use crate::tool::Tool;

#[derive(Clone)]
pub struct Agent {
    pub name: AgentName,
    pub description: String,
    pub instruction: String,
    pub kind: AgentKind,
    pub model: Arc<dyn LanguageModel>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub sub_agents: Vec<Agent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentKind {
    Llm,
    Sequential,
    Parallel,
    Loop { max_iterations: u32 },
}

pub struct AgentBuilder {
    name: AgentName,
    description: String,
    instruction: String,
    kind: AgentKind,
    model: Arc<dyn LanguageModel>,
    tools: Vec<Arc<dyn Tool>>,
    sub_agents: Vec<Agent>,
}

impl AgentBuilder {
    pub fn new(
        name: AgentName,
        instruction: impl Into<String>,
        model: Arc<dyn LanguageModel>,
    ) -> Self {
        Self {
            name,
            description: String::new(),
            instruction: instruction.into(),
            kind: AgentKind::Llm,
            model,
            tools: Vec::new(),
            sub_agents: Vec::new(),
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn kind(mut self, kind: AgentKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn sequential(self) -> Self {
        self.kind(AgentKind::Sequential)
    }

    pub fn parallel(self) -> Self {
        self.kind(AgentKind::Parallel)
    }

    pub fn loop_agent(self, max_iterations: u32) -> Self {
        self.kind(AgentKind::Loop { max_iterations })
    }

    pub fn tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn sub_agent(mut self, agent: Agent) -> Self {
        self.sub_agents.push(agent);
        self
    }

    pub fn build(self) -> Result<Agent, AgentError> {
        if self.instruction.trim().is_empty() {
            return Err(AgentError::EmptyInstruction);
        }
        Ok(Agent {
            name: self.name,
            description: self.description,
            instruction: self.instruction,
            kind: self.kind,
            model: self.model,
            tools: self.tools,
            sub_agents: self.sub_agents,
        })
    }
}

impl Agent {
    pub fn find_agent(&self, name: &AgentName) -> Option<&Agent> {
        if &self.name == name {
            return Some(self);
        }
        self.sub_agents
            .iter()
            .find_map(|agent| agent.find_agent(name))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AgentError {
    #[error("agent instruction cannot be empty")]
    EmptyInstruction,
}
