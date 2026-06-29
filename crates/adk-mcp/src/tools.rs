//! The only *executable* tools adk-mcp ships. adk-rs has no FunctionTool/closure
//! helper and its `BuiltinToolKind` specs are declaration-only (not runnable),
//! so we hand-write real `impl Tool` structs and resolve them by name.

use std::sync::Arc;

use adk_rs::{Tool, ToolCall, ToolError, ToolResult, ToolSpec};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Counts whitespace-separated words in `text`.
struct WordCountTool;

#[async_trait]
impl Tool for WordCountTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "word_count".to_owned(),
            description: "Count the words in a text string.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let text = call
            .args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed {
                name: "word_count".to_owned(),
                message: "missing string field `text`".to_owned(),
            })?;
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "words": text.split_whitespace().count() }),
        })
    }
}

/// Returns the current UTC time as Unix-epoch seconds.
struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "current_time".to_owned(),
            description: "Get the current time as Unix-epoch seconds.".to_owned(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "epoch_secs": secs }),
        })
    }
}

/// (name, description) of every executable tool we ship.
pub const EXECUTABLE_TOOLS: &[(&str, &str)] = &[
    ("word_count", "Count the words in a text string."),
    ("current_time", "Get the current time as Unix-epoch seconds."),
];

/// Resolve a tool name to a runnable `Arc<dyn Tool>`, or `None` if unknown.
pub fn resolve_tool(name: &str) -> Option<Arc<dyn Tool>> {
    match name {
        "word_count" => Some(Arc::new(WordCountTool)),
        "current_time" => Some(Arc::new(CurrentTimeTool)),
        _ => None,
    }
}
