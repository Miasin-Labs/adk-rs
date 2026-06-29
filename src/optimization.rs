use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationCandidate {
    pub prompt: String,
    pub score: f64,
}

#[async_trait]
pub trait Optimizer: Send + Sync {
    async fn optimize(&self, prompt: &str) -> Result<OptimizationCandidate, OptimizerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum OptimizerError {
    #[error("optimization failed: {0}")]
    Failed(String),
}
