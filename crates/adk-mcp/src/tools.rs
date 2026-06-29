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

/// Make an HTTP request to a runtime-supplied URL and return the response.
struct HttpRequestTool;

#[async_trait]
impl Tool for HttpRequestTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "http_request".to_owned(),
            description: "Make an HTTP request to a URL and return its status and body.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "method": { "type": "string", "enum": ["GET", "POST", "PUT", "DELETE"] },
                    "url": { "type": "string" },
                    "body": { "type": "object" }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let url = call
            .args
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .ok_or_else(|| ToolError::Failed {
                name: "http_request".to_owned(),
                message: "missing string field `url`".to_owned(),
            })?;
        let method = call.args.get("method").and_then(Value::as_str).unwrap_or("GET");
        let client = reqwest::Client::new();
        let mut request = match method.to_ascii_uppercase().as_str() {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            _ => client.get(url),
        }
        .timeout(std::time::Duration::from_secs(30))
        // Many APIs (e.g. GitHub) reject requests without a User-Agent.
        .header("User-Agent", concat!("adk-mcp/", env!("CARGO_PKG_VERSION")));
        if let Some(body) = call.args.get("body") {
            request = request.json(body);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return Ok(ToolResult {
                    call_id: call.id.clone(),
                    content: json!({ "error": format!("request failed: {error}") }),
                });
            }
        };
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!(text));
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "statusCode": status, "body": body }),
        })
    }
}

/// (name, description) of every executable tool we ship.
pub const EXECUTABLE_TOOLS: &[(&str, &str)] = &[
    ("word_count", "Count the words in a text string."),
    ("current_time", "Get the current time as Unix-epoch seconds."),
    ("http_request", "Make an HTTP request to a URL and return its status and body."),
];

/// Resolve a tool name to a runnable `Arc<dyn Tool>`, or `None` if unknown.
pub fn resolve_tool(name: &str) -> Option<Arc<dyn Tool>> {
    match name {
        "word_count" => Some(Arc::new(WordCountTool)),
        "current_time" => Some(Arc::new(CurrentTimeTool)),
        "http_request" => Some(Arc::new(HttpRequestTool)),
        _ => None,
    }
}
