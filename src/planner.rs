use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::invocation::InvocationContext;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}

#[async_trait]
pub trait Planner: Send + Sync {
    async fn build_plan(
        &self,
        context: &InvocationContext,
        task: &str,
    ) -> Result<Plan, PlannerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    #[error("planner failed: {0}")]
    Failed(String),
}
