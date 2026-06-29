use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[async_trait]
pub trait CodeExecutor: Send + Sync {
    async fn execute(&self, block: CodeBlock) -> Result<CodeExecutionResult, CodeExecutorError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CodeExecutorError {
    #[error("unsupported language {0}")]
    UnsupportedLanguage(String),
    #[error("code execution failed: {0}")]
    Failed(String),
}
