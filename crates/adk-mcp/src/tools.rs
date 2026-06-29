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

/// Evaluate a math expression and return the numeric result.
struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "calculator".to_owned(),
            description: "Evaluate a math expression, e.g. \"2 * (3 + 4)\", and return the result."
                .to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "expression": { "type": "string" } },
                "required": ["expression"]
            }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let expression = call.args.get("expression").and_then(Value::as_str).unwrap_or("");
        let content = match evalexpr::eval(expression) {
            Ok(evalexpr::Value::Int(number)) => json!({ "result": number }),
            Ok(evalexpr::Value::Float(number)) => json!({ "result": number }),
            Ok(evalexpr::Value::Boolean(value)) => json!({ "result": value }),
            Ok(other) => json!({ "result": other.to_string() }),
            Err(error) => json!({ "error": error.to_string() }),
        };
        Ok(ToolResult {
            call_id: call.id.clone(),
            content,
        })
    }
}

/// Fetch a web page and return its readable text (HTML stripped).
struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "web_fetch".to_owned(),
            description: "Fetch a web page and return its readable text content (HTML stripped)."
                .to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "url": { "type": "string" } },
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
                name: "web_fetch".to_owned(),
                message: "missing string field `url`".to_owned(),
            })?;
        let response = match reqwest::Client::new()
            .get(url)
            .timeout(std::time::Duration::from_secs(30))
            .header("User-Agent", USER_AGENT)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return Ok(ToolResult {
                    call_id: call.id.clone(),
                    content: json!({ "error": format!("request failed: {error}") }),
                });
            }
        };
        let status = response.status().as_u16();
        let html = response.text().await.unwrap_or_default();
        let text = html_to_text(&html);
        let truncated: String = text.chars().take(8000).collect();
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({
                "statusCode": status,
                "text": truncated,
                "truncated": text.chars().count() > 8000
            }),
        })
    }
}

const USER_AGENT: &str = concat!("adk-mcp/", env!("CARGO_PKG_VERSION"));

/// Remove `<tag>…</tag>` blocks (e.g. script/style) wholesale.
fn remove_tag_blocks(input: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let lower = input.to_ascii_lowercase();
    let mut out = String::new();
    let mut index = 0;
    while index < input.len() {
        match lower[index..].find(&open) {
            Some(offset) => {
                let start = index + offset;
                out.push_str(&input[index..start]);
                index = match lower[start..].find(&close) {
                    Some(end) => start + end + close.len(),
                    None => input.len(),
                };
            }
            None => {
                out.push_str(&input[index..]);
                break;
            }
        }
    }
    out
}

/// Minimal HTML → readable text: drop script/style, turn block tags into line
/// breaks, strip remaining tags, decode common entities, and tidy whitespace.
fn html_to_text(html: &str) -> String {
    let cleaned = remove_tag_blocks(&remove_tag_blocks(html, "script"), "style");
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for character in cleaned.chars() {
        match character {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' => {
                in_tag = false;
                let name = tag
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "br" | "p" | "div" | "li" | "tr" | "ul" | "ol" | "section" | "article"
                        | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                ) {
                    out.push('\n');
                }
            }
            _ if in_tag => tag.push(character),
            _ => out.push(character),
        }
    }
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// (name, description) of every executable tool we ship.
pub const EXECUTABLE_TOOLS: &[(&str, &str)] = &[
    ("word_count", "Count the words in a text string."),
    ("current_time", "Get the current time as Unix-epoch seconds."),
    ("http_request", "Make an HTTP request to a URL and return its status and body."),
    ("calculator", "Evaluate a math expression and return the result."),
    ("web_fetch", "Fetch a web page and return its readable text content."),
];

/// Resolve a tool name to a runnable `Arc<dyn Tool>`, or `None` if unknown.
pub fn resolve_tool(name: &str) -> Option<Arc<dyn Tool>> {
    match name {
        "word_count" => Some(Arc::new(WordCountTool)),
        "current_time" => Some(Arc::new(CurrentTimeTool)),
        "http_request" => Some(Arc::new(HttpRequestTool)),
        "calculator" => Some(Arc::new(CalculatorTool)),
        "web_fetch" => Some(Arc::new(WebFetchTool)),
        _ => None,
    }
}
