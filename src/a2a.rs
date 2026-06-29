use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::ids::AgentName;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aAgentCard {
    pub name: AgentName,
    pub endpoint: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aMessage {
    pub task_id: String,
    pub role: String,
    pub text: String,
}

#[async_trait]
pub trait A2aTransport: Send + Sync {
    async fn send_message(
        &self,
        card: &A2aAgentCard,
        message: A2aMessage,
    ) -> Result<A2aMessage, A2aError>;
}

#[derive(Clone)]
pub struct RemoteA2aAgent<T: A2aTransport> {
    pub card: A2aAgentCard,
    pub transport: T,
}

impl<T: A2aTransport> RemoteA2aAgent<T> {
    pub async fn invoke(&self, message: A2aMessage) -> Result<A2aMessage, A2aError> {
        self.transport.send_message(&self.card, message).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum A2aError {
    #[error("A2A transport failed: {0}")]
    Transport(String),
}
