use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Command;

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

/// A built-in code executor that runs code blocks in subprocesses.
#[derive(Debug, Default)]
pub struct LocalCodeExecutor;

impl LocalCodeExecutor {
    /// Creates a new `LocalCodeExecutor`.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CodeExecutor for LocalCodeExecutor {
    async fn execute(&self, block: CodeBlock) -> Result<CodeExecutionResult, CodeExecutorError> {
        let language = block.language.to_lowercase();
        let code = block.code.clone();

        // Determine the command and arguments based on language
        let (cmd, arg) = match language.as_str() {
            "python" | "python3" => ("python3", "-c"),
            "bash" | "sh" => ("bash", "-c"),
            _ => {
                return Err(CodeExecutorError::UnsupportedLanguage(block.language));
            }
        };

        // Run the command in a blocking task to avoid blocking the async executor
        tokio::task::spawn_blocking(move || {
            let output = Command::new(cmd)
                .arg(arg)
                .arg(&code)
                .output()
                .map_err(|e| CodeExecutorError::Failed(format!("failed to spawn process: {}", e)))?;

            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit_code = output.status.code().unwrap_or(-1);

            Ok(CodeExecutionResult {
                stdout,
                stderr,
                exit_code,
            })
        })
        .await
        .map_err(|e| CodeExecutorError::Failed(format!("task join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_code_executor_python_basic() {
        let executor = LocalCodeExecutor::new();
        let block = CodeBlock {
            language: "python3".to_string(),
            code: "print('hello from python')".to_string(),
        };

        let result = executor.execute(block).await;

        match result {
            Ok(output) => {
                assert_eq!(output.stdout, "hello from python\n");
                assert_eq!(output.exit_code, 0);
            }
            Err(CodeExecutorError::UnsupportedLanguage(_)) => {
                // Python not available, skip this test
            }
            Err(e) => panic!("unexpected error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_code_executor_bash_basic() {
        let executor = LocalCodeExecutor::new();
        let block = CodeBlock {
            language: "bash".to_string(),
            code: "echo 'hello from bash'".to_string(),
        };

        let result = executor.execute(block).await;

        match result {
            Ok(output) => {
                assert_eq!(output.stdout, "hello from bash\n");
                assert_eq!(output.exit_code, 0);
            }
            Err(e) => panic!("unexpected error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_code_executor_unsupported_language() {
        let executor = LocalCodeExecutor::new();
        let block = CodeBlock {
            language: "rust".to_string(),
            code: "fn main() {}".to_string(),
        };

        let result = executor.execute(block).await;

        assert!(matches!(
            result,
            Err(CodeExecutorError::UnsupportedLanguage(ref lang)) if lang == "rust"
        ));
    }

    #[tokio::test]
    async fn test_code_executor_default() {
        let executor = LocalCodeExecutor::default();
        let block = CodeBlock {
            language: "bash".to_string(),
            code: "true".to_string(),
        };

        let result = executor.execute(block).await;
        assert!(result.is_ok());
    }
}
