use async_trait::async_trait;
use reqwest::Url;
use serde_json::{Value, json};

use crate::event::{Event, EventAuthor, EventPart};
use crate::model::{LanguageModel, ModelError, ModelRequest, ModelResponse};
use crate::tool::{ToolCall, ToolSpec};

/// Configuration for the native Anthropic Messages API adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicConfig {
    /// Base URL for the Anthropic API. Defaults to `https://api.anthropic.com`.
    pub base_url: String,
    /// The Anthropic model identifier, e.g. `claude-3-5-sonnet-20241022`.
    pub model: String,
    /// Anthropic API key (`x-api-key` header).
    pub api_key: String,
    /// Maximum number of tokens to generate.
    pub max_tokens: u32,
}

impl AnthropicConfig {
    /// Create a config targeting the official Anthropic API endpoint.
    pub fn new(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_owned(),
            model: model.into(),
            api_key: api_key.into(),
            max_tokens: 4096,
        }
    }
}

/// A [`LanguageModel`] adapter for the Anthropic Messages API.
#[derive(Clone)]
pub struct AnthropicModel {
    config: AnthropicConfig,
    client: reqwest::Client,
}

impl AnthropicModel {
    /// Construct a new adapter, validating the `base_url`.
    pub fn new(config: AnthropicConfig) -> Result<Self, ModelError> {
        Url::parse(&config.base_url).map_err(|e| ModelError::Failed(e.to_string()))?;
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl LanguageModel for AnthropicModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let url = messages_url(&self.config.base_url)?;
        let body = build_request_body(&self.config.model, self.config.max_tokens, &request);

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ModelError::Failed(e.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| ModelError::Failed(e.to_string()))?;

        if !status.is_success() {
            return Err(ModelError::Failed(format!(
                "Anthropic Messages API returned {status}: {text}"
            )));
        }

        let value: Value = serde_json::from_str(&text)
            .map_err(|e| ModelError::Failed(e.to_string()))?;

        parse_messages_response(&value)
    }
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

fn messages_url(base_url: &str) -> Result<Url, ModelError> {
    let base = if base_url.ends_with('/') {
        base_url.to_owned()
    } else {
        format!("{base_url}/")
    };
    let base = Url::parse(&base).map_err(|e| ModelError::Failed(e.to_string()))?;
    base.join("v1/messages")
        .map_err(|e| ModelError::Failed(e.to_string()))
}

fn build_request_body(model: &str, max_tokens: u32, request: &ModelRequest) -> Value {
    let messages: Vec<Value> = request.events.iter().flat_map(event_to_messages).collect();
    let tools: Vec<Value> = request.tools.iter().map(tool_schema).collect();

    json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": request.instruction,
        "messages": messages,
        "tools": tools,
    })
}

/// Convert an ADK [`Event`] into one or more Anthropic message objects.
///
/// Anthropic's Messages API uses a `content` array on each message.
/// Tool calls are expressed as `tool_use` blocks on an `assistant` message;
/// tool results are expressed as `tool_result` blocks on a `user` message.
fn event_to_messages(event: &Event) -> Vec<Value> {
    let mut messages = Vec::new();

    // Collect text and tool-call blocks for the primary message.
    let mut content_blocks: Vec<Value> = Vec::new();

    for part in &event.parts {
        match part {
            EventPart::Text(text) => {
                content_blocks.push(json!({ "type": "text", "text": text }));
            }
            EventPart::ToolCall(call) => {
                content_blocks.push(json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": call.args,
                }));
            }
            EventPart::ToolResult(_) => {} // handled below
        }
    }

    if !content_blocks.is_empty() {
        let role = author_role(&event.author);
        messages.push(json!({ "role": role, "content": content_blocks }));
    }

    // Tool results become a separate `user` message (Anthropic requirement).
    let result_blocks: Vec<Value> = event
        .parts
        .iter()
        .filter_map(|part| {
            if let EventPart::ToolResult(result) = part {
                Some(json!({
                    "type": "tool_result",
                    "tool_use_id": result.call_id,
                    "content": result.content.to_string(),
                }))
            } else {
                None
            }
        })
        .collect();

    if !result_blocks.is_empty() {
        messages.push(json!({ "role": "user", "content": result_blocks }));
    }

    messages
}

fn author_role(author: &EventAuthor) -> &'static str {
    match author {
        EventAuthor::User => "user",
        EventAuthor::Agent(_) => "assistant",
        EventAuthor::Tool(_) => "user",
    }
}

fn tool_schema(spec: &ToolSpec) -> Value {
    json!({
        "name": spec.name,
        "description": spec.description,
        "input_schema": spec.input_schema,
    })
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

fn parse_messages_response(value: &Value) -> Result<ModelResponse, ModelError> {
    let content = value
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ModelError::Failed("missing content array in Anthropic response".to_owned()))?;

    let mut text: Option<String> = None;
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = block
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                {
                    text = Some(t.to_owned());
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ModelError::Failed("tool_use block missing id".to_owned()))?
                    .to_owned();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ModelError::Failed("tool_use block missing name".to_owned()))?
                    .to_owned();
                let args = block
                    .get("input")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                tool_calls.push(ToolCall { id, name, args });
            }
            _ => {} // ignore unknown block types
        }
    }

    Ok(ModelResponse {
        text,
        tool_calls,
        actions: Default::default(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use serde_json::json;

    use crate::event::{Event, EventAuthor, EventPart};
    use crate::ids::{EventId, InvocationId};
    use crate::model::{LanguageModel, ModelRequest, ModelResponse};
    use crate::tool::{ToolCall, ToolSpec};

    use super::{AnthropicConfig, AnthropicModel};

    /// Spin up a minimal raw-TCP HTTP/1.1 server that:
    /// 1. Accepts exactly one connection.
    /// 2. Reads the request into a `String` and calls `assert_fn` on it.
    /// 3. Writes the provided `response_body` back as a 200 JSON response.
    fn fake_server(
        response_body: String,
        assert_fn: impl FnOnce(&str) + Send + 'static,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert_fn(&req);
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(resp.as_bytes()).unwrap();
        });
        addr
    }

    /// Build a minimal `AnthropicModel` pointing at a local fake server.
    fn model_at(addr: std::net::SocketAddr) -> AnthropicModel {
        AnthropicModel::new(AnthropicConfig {
            base_url: format!("http://{addr}"),
            model: "claude-test".to_owned(),
            api_key: "test-anthropic-key".to_owned(),
            max_tokens: 1024,
        })
        .unwrap()
    }

    // -----------------------------------------------------------------------
    // anthropic_text_response_parsed
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn anthropic_text_response_parsed() {
        let body = json!({
            "id": "msg_01",
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "text", "text": "Hello from Claude!" }
            ],
            "model": "claude-test",
            "stop_reason": "end_turn"
        })
        .to_string();

        let addr = fake_server(body, |req| {
            assert!(req.starts_with("POST /v1/messages HTTP/1.1"), "unexpected path: {req}");
            assert!(req.contains("x-api-key: test-anthropic-key"), "missing api key header");
            assert!(req.contains("anthropic-version: 2023-06-01"), "missing version header");
            assert!(req.contains("\"model\":\"claude-test\""), "missing model field");
        });

        let response = model_at(addr)
            .generate(ModelRequest {
                instruction: "Be helpful.".to_owned(),
                events: vec![Event {
                    id: EventId::for_index(0),
                    invocation_id: InvocationId::new("turn-1").unwrap(),
                    author: EventAuthor::User,
                    parts: vec![EventPart::Text("Hello!".to_owned())],
                    actions: crate::event::EventActions::default(),
                    timestamp_seconds: 0,
                }],
                tools: vec![],
            })
            .await
            .unwrap();

        assert_eq!(
            response,
            ModelResponse {
                text: Some("Hello from Claude!".to_owned()),
                tool_calls: vec![],
                actions: Default::default(),
            }
        );
    }

    // -----------------------------------------------------------------------
    // anthropic_tool_use_and_text_response_parsed
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn anthropic_tool_use_and_text_response_parsed() {
        let body = json!({
            "id": "msg_02",
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "text", "text": "I will look that up for you." },
                {
                    "type": "tool_use",
                    "id": "toolu_01",
                    "name": "search",
                    "input": { "query": "Rust programming language" }
                }
            ],
            "model": "claude-test",
            "stop_reason": "tool_use"
        })
        .to_string();

        let addr = fake_server(body, |req| {
            assert!(req.starts_with("POST /v1/messages HTTP/1.1"), "unexpected path: {req}");
            assert!(req.contains("x-api-key: test-anthropic-key"), "missing api key header");
            assert!(req.contains("anthropic-version: 2023-06-01"), "missing version header");
            // System prompt should be in body
            assert!(req.contains("\"system\""), "missing system field");
            // Tool schema forwarded
            assert!(req.contains("\"name\":\"search\""), "tool name not forwarded");
        });

        let tool_spec = ToolSpec {
            name: "search".to_owned(),
            description: "Search the web".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        };

        let response = model_at(addr)
            .generate(ModelRequest {
                instruction: "Use tools when needed.".to_owned(),
                events: vec![Event {
                    id: EventId::for_index(1),
                    invocation_id: InvocationId::new("turn-2").unwrap(),
                    author: EventAuthor::User,
                    parts: vec![EventPart::Text("Find Rust.".to_owned())],
                    actions: crate::event::EventActions::default(),
                    timestamp_seconds: 0,
                }],
                tools: vec![tool_spec],
            })
            .await
            .unwrap();

        assert_eq!(
            response,
            ModelResponse {
                text: Some("I will look that up for you.".to_owned()),
                tool_calls: vec![ToolCall {
                    id: "toolu_01".to_owned(),
                    name: "search".to_owned(),
                    args: json!({ "query": "Rust programming language" }),
                }],
                actions: Default::default(),
            }
        );
    }

    // -----------------------------------------------------------------------
    // anthropic_tool_use_only_no_text
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn anthropic_tool_use_only_no_text() {
        let body = json!({
            "id": "msg_03",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_02",
                    "name": "compute",
                    "input": { "expression": "2+2" }
                }
            ],
            "model": "claude-test",
            "stop_reason": "tool_use"
        })
        .to_string();

        let addr = fake_server(body, |_req| {});

        let response = model_at(addr)
            .generate(ModelRequest {
                instruction: "Compute.".to_owned(),
                events: vec![],
                tools: vec![],
            })
            .await
            .unwrap();

        assert_eq!(response.text, None);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_02");
        assert_eq!(response.tool_calls[0].name, "compute");
        assert_eq!(response.tool_calls[0].args, json!({ "expression": "2+2" }));
    }

    // -----------------------------------------------------------------------
    // anthropic_http_error_propagated
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn anthropic_http_error_propagated() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 4096];
            let _ = stream.read(&mut buf).unwrap();
            let error_body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
            let resp = format!(
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                error_body.len(),
                error_body
            );
            stream.write_all(resp.as_bytes()).unwrap();
        });

        let result = model_at(addr)
            .generate(ModelRequest {
                instruction: "test".to_owned(),
                events: vec![],
                tools: vec![],
            })
            .await;

        assert!(result.is_err(), "expected an error for 401 response");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("401"), "error should mention status code: {err}");
    }
}
