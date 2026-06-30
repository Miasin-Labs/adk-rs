use async_trait::async_trait;
use reqwest::Url;
use serde_json::{Value, json};

use crate::event::{Event, EventAuthor, EventPart};
use crate::model::{LanguageModel, ModelError, ModelRequest, ModelResponse};
use crate::tool::{ToolCall, ToolSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Clone)]
pub struct GeminiModel {
    config: GeminiConfig,
    client: reqwest::Client,
}

impl GeminiModel {
    pub fn new(config: GeminiConfig) -> Result<Self, ModelError> {
        Url::parse(&config.base_url).map_err(|source| ModelError::Failed(source.to_string()))?;
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl LanguageModel for GeminiModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let url = generate_content_url(&self.config.base_url, &self.config.model, &self.config.api_key)?;
        
        let request_body = generate_content_request(&self.config.model, &request);
        
        let response = self
            .client
            .post(url)
            .json(&request_body)
            .send()
            .await
            .map_err(|source| ModelError::Failed(source.to_string()))?;
        
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|source| ModelError::Failed(source.to_string()))?;
        
        if !status.is_success() {
            return Err(ModelError::Failed(format!(
                "Gemini model returned {status}: {body}"
            )));
        }
        
        let value = serde_json::from_str::<Value>(&body)
            .map_err(|source| ModelError::Failed(source.to_string()))?;
        
        parse_generate_content_response(&value)
    }
}

fn generate_content_url(
    base_url: &str,
    model: &str,
    api_key: &str,
) -> Result<Url, ModelError> {
    let base_url = if base_url.ends_with('/') {
        base_url.to_owned()
    } else {
        format!("{base_url}/")
    };
    let base = Url::parse(&base_url).map_err(|source| ModelError::Failed(source.to_string()))?;
    base.join(&format!("v1beta/models/{model}:generateContent"))
        .map_err(|source| ModelError::Failed(source.to_string()))?
        .join(&format!("?key={api_key}"))
        .map_err(|source| ModelError::Failed(source.to_string()))
}

fn generate_content_request(_model: &str, request: &ModelRequest) -> Value {
    let mut contents = vec![json!({
        "role": "user",
        "parts": [
            { "text": request.instruction }
        ]
    })];
    
    // Add event-based contents
    contents.extend(request.events.iter().flat_map(event_contents));
    
    let mut body = json!({
        "contents": contents,
    });
    
    // Add tools if present
    if !request.tools.is_empty() {
        body["tools"] = json!([
            {
                "functionDeclarations": request.tools.iter().map(tool_declaration).collect::<Vec<_>>()
            }
        ]);
    }
    
    body
}

fn event_contents(event: &Event) -> Vec<Value> {
    let mut contents = Vec::new();
    
    // Collect text and tool calls that belong together
    let mut parts = Vec::new();
    let mut role = role_from_author(&event.author);
    
    for part in &event.parts {
        match part {
            EventPart::Text(text) => {
                parts.push(json!({ "text": text }));
            }
            EventPart::ToolCall(call) => {
                parts.push(json!({
                    "functionCall": {
                        "name": call.name,
                        "args": call.args
                    }
                }));
                // Tool calls come from the model
                role = "model";
            }
            EventPart::ToolResult(result) => {
                // Tool results come from the user/system
                if !parts.is_empty() {
                    contents.push(json!({
                        "role": role,
                        "parts": parts
                    }));
                    parts = Vec::new();
                }
                contents.push(json!({
                    "role": "user",
                    "parts": [
                        {
                            "functionResponse": {
                                "name": "unknown", // We don't have the original tool name in ToolResult
                                "response": result.content
                            }
                        }
                    ]
                }));
                role = role_from_author(&event.author);
            }
        }
    }
    
    // Add any remaining parts
    if !parts.is_empty() {
        contents.push(json!({
            "role": role,
            "parts": parts
        }));
    }
    
    contents
}

fn role_from_author(author: &EventAuthor) -> &'static str {
    match author {
        EventAuthor::User => "user",
        EventAuthor::Agent(_) => "model",
        EventAuthor::Tool(_) => "user",
    }
}

fn tool_declaration(spec: &ToolSpec) -> Value {
    json!({
        "name": spec.name,
        "description": spec.description,
        "parameters": spec.input_schema,
    })
}

fn parse_generate_content_response(value: &Value) -> Result<ModelResponse, ModelError> {
    let candidates = value
        .get("candidates")
        .and_then(Value::as_array)
        .ok_or_else(|| ModelError::Failed("missing candidates array".to_owned()))?;
    
    let candidate = candidates
        .first()
        .ok_or_else(|| ModelError::Failed("empty candidates array".to_owned()))?;
    
    let content = candidate
        .get("content")
        .ok_or_else(|| ModelError::Failed("missing content in candidate".to_owned()))?;
    
    let parts = content
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| ModelError::Failed("missing parts in content".to_owned()))?;
    
    let mut text = None;
    let mut tool_calls = Vec::new();
    
    for part in parts {
        if let Some(text_content) = part.get("text").and_then(Value::as_str) {
            if !text_content.is_empty() {
                text = Some(text_content.to_owned());
            }
        } else if let Some(function_call) = part.get("functionCall") {
            tool_calls.push(parse_function_call(function_call)?);
        }
    }
    
    Ok(ModelResponse {
        text,
        tool_calls,
        actions: Default::default(),
    })
}

fn parse_function_call(value: &Value) -> Result<ToolCall, ModelError> {
    let name = required_string(value, "name")?.to_owned();
    let args = value
        .get("args")
        .cloned()
        .unwrap_or_else(|| json!({}));
    
    // Generate a simple ID based on the name
    let id = format!("call_{}", uuid_like_string());
    
    Ok(ToolCall { id, name, args })
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, ModelError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ModelError::Failed(format!("missing string field {field}")))
}

fn uuid_like_string() -> String {
    // Simple deterministic UUID-like generation for testing
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:08x}", nanos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[tokio::test]
    async fn test_gemini_model_with_text_and_function_call() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to get local addr");
        let port = addr.port();

        // Spawn a fake server in a thread
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = vec![0; 4096];
                let _n = stream.read(&mut buffer).expect("failed to read request");

                let response = json!({
                    "candidates": [
                        {
                            "content": {
                                "parts": [
                                    {
                                        "text": "This is a response from Gemini"
                                    },
                                    {
                                        "functionCall": {
                                            "name": "calculate_sum",
                                            "args": {
                                                "a": 5,
                                                "b": 3
                                            }
                                        }
                                    }
                                ]
                            }
                        }
                    ]
                });

                let body = serde_json::to_string(&response).unwrap();
                let http_response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(http_response.as_bytes());
            }
        });

        let config = GeminiConfig {
            base_url: format!("http://127.0.0.1:{}", port),
            model: "gemini-pro".to_string(),
            api_key: "test-key".to_string(),
        };

        let model = GeminiModel::new(config).expect("failed to create model");
        let request = ModelRequest {
            instruction: "Test instruction".to_string(),
            events: vec![],
            tools: vec![],
        };

        let response = model.generate(request).await.expect("failed to generate");

        assert_eq!(response.text, Some("This is a response from Gemini".to_string()));
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "calculate_sum");
        assert_eq!(response.tool_calls[0].args.get("a").and_then(Value::as_i64), Some(5));
        assert_eq!(response.tool_calls[0].args.get("b").and_then(Value::as_i64), Some(3));
    }

    #[tokio::test]
    async fn test_gemini_model_with_events() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to get local addr");
        let port = addr.port();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = vec![0; 4096];
                let _n = stream.read(&mut buffer).expect("failed to read request");

                let response = json!({
                    "candidates": [
                        {
                            "content": {
                                "parts": [
                                    {
                                        "text": "Response with event handling"
                                    }
                                ]
                            }
                        }
                    ]
                });

                let body = serde_json::to_string(&response).unwrap();
                let http_response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(http_response.as_bytes());
            }
        });

        let config = GeminiConfig {
            base_url: format!("http://127.0.0.1:{}", port),
            model: "gemini-pro".to_string(),
            api_key: "test-key".to_string(),
        };

        let model = GeminiModel::new(config).expect("failed to create model");

        let invocation_id = crate::ids::InvocationId::new("inv-1").expect("failed to create invocation id");
        let user_event = Event::text(
            invocation_id,
            EventAuthor::User,
            "What is 5 + 3?",
        );

        let request = ModelRequest {
            instruction: "You are a math assistant.".to_string(),
            events: vec![user_event],
            tools: vec![],
        };

        let response = model.generate(request).await.expect("failed to generate");

        assert_eq!(response.text, Some("Response with event handling".to_string()));
        assert_eq!(response.tool_calls.len(), 0);
    }

    #[tokio::test]
    async fn test_gemini_model_handles_http_error() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to get local addr");
        let port = addr.port();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = vec![0; 4096];
                let _n = stream.read(&mut buffer).expect("failed to read request");

                let error_response = r#"{"error": {"message": "Invalid API key"}}"#;
                let http_response = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\n\r\n{}",
                    error_response.len(),
                    error_response
                );
                let _ = stream.write_all(http_response.as_bytes());
            }
        });

        let config = GeminiConfig {
            base_url: format!("http://127.0.0.1:{}", port),
            model: "gemini-pro".to_string(),
            api_key: "invalid-key".to_string(),
        };

        let model = GeminiModel::new(config).expect("failed to create model");
        let request = ModelRequest {
            instruction: "Test".to_string(),
            events: vec![],
            tools: vec![],
        };

        let result = model.generate(request).await;
        assert!(result.is_err());
    }
}
