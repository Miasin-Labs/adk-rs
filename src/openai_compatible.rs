use async_trait::async_trait;
use reqwest::Url;
use serde_json::{Value, json};

use crate::auth::AuthCredential;
use crate::event::{Event, EventAuthor, EventPart};
use crate::model::{LanguageModel, ModelError, ModelRequest, ModelResponse};
use crate::tool::{ToolCall, ToolSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub model: String,
    pub credential: AuthCredential,
}

#[derive(Clone)]
pub struct OpenAiCompatibleModel {
    config: OpenAiCompatibleConfig,
    client: reqwest::Client,
}

impl OpenAiCompatibleModel {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, ModelError> {
        Url::parse(&config.base_url).map_err(|source| ModelError::Failed(source.to_string()))?;
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl LanguageModel for OpenAiCompatibleModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let response = self
            .client
            .post(chat_completions_url(&self.config.base_url)?)
            .bearer_auth(bearer_token(&self.config.credential)?)
            .json(&chat_request(&self.config.model, &request))
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
                "OpenAI-compatible model returned {status}: {body}"
            )));
        }
        let value = serde_json::from_str::<Value>(&body)
            .map_err(|source| ModelError::Failed(source.to_string()))?;
        parse_chat_response(&value)
    }
}

fn chat_completions_url(base_url: &str) -> Result<Url, ModelError> {
    let base_url = if base_url.ends_with('/') {
        base_url.to_owned()
    } else {
        format!("{base_url}/")
    };
    let base = Url::parse(&base_url).map_err(|source| ModelError::Failed(source.to_string()))?;
    base.join("chat/completions")
        .map_err(|source| ModelError::Failed(source.to_string()))
}

fn bearer_token(credential: &AuthCredential) -> Result<&str, ModelError> {
    match credential {
        AuthCredential::ApiKey(secret)
        | AuthCredential::BearerToken(secret)
        | AuthCredential::OAuth2 {
            access_token: secret,
            ..
        } => Ok(secret),
        AuthCredential::ServiceAccountJson(_) => Err(ModelError::UnsupportedCapability(
            "service-account credentials are not supported for OpenAI-compatible chat".to_owned(),
        )),
    }
}

fn chat_request(model: &str, request: &ModelRequest) -> Value {
    let mut messages = vec![json!({
        "role": "system",
        "content": request.instruction,
    })];
    messages.extend(request.events.iter().flat_map(event_messages));
    json!({
        "model": model,
        "messages": messages,
        "tools": request.tools.iter().map(tool_schema).collect::<Vec<_>>(),
    })
}

fn event_messages(event: &Event) -> Vec<Value> {
    let mut messages = Vec::new();

    // Tool calls must ride on a single `assistant` message (with `tool_calls`)
    // that precedes the matching `tool` result messages — otherwise the API
    // rejects the request.
    let tool_calls: Vec<Value> = event
        .parts
        .iter()
        .filter_map(|part| match part {
            EventPart::ToolCall(call) => Some(json!({
                "id": call.id,
                "type": "function",
                "function": { "name": call.name, "arguments": call.args.to_string() },
            })),
            _ => None,
        })
        .collect();
    let text = event.parts.iter().find_map(|part| match part {
        EventPart::Text(text) => Some(text.clone()),
        _ => None,
    });

    if !tool_calls.is_empty() {
        // `content` is null when the assistant only calls tools.
        messages.push(json!({
            "role": "assistant",
            "content": text,
            "tool_calls": tool_calls,
        }));
    } else if let Some(text) = text {
        messages.push(json!({
            "role": author_role(&event.author),
            "content": text,
        }));
    }

    for part in &event.parts {
        if let EventPart::ToolResult(result) = part {
            messages.push(json!({
                "role": "tool",
                "tool_call_id": result.call_id,
                "content": result.content.to_string(),
            }));
        }
    }

    messages
}

fn author_role(author: &EventAuthor) -> &'static str {
    match author {
        EventAuthor::User => "user",
        EventAuthor::Agent(_) => "assistant",
        EventAuthor::Tool(_) => "tool",
    }
}

fn tool_schema(spec: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": spec.input_schema,
        }
    })
}

fn parse_chat_response(value: &Value) -> Result<ModelResponse, ModelError> {
    let message = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| ModelError::Failed("missing chat completion message".to_owned()))?;
    Ok(ModelResponse {
        text: message
            .get("content")
            .and_then(Value::as_str)
            .filter(|content| !content.is_empty())
            .map(str::to_owned),
        tool_calls: parse_tool_calls(message)?,
        actions: Default::default(),
    })
}

fn parse_tool_calls(message: &Value) -> Result<Vec<ToolCall>, ModelError> {
    let Some(calls) = message.get("tool_calls").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    calls.iter().map(parse_tool_call).collect()
}

fn parse_tool_call(value: &Value) -> Result<ToolCall, ModelError> {
    let function = value
        .get("function")
        .ok_or_else(|| ModelError::Failed("missing tool call function".to_owned()))?;
    let args = function
        .get("arguments")
        .and_then(Value::as_str)
        .map(parse_arguments)
        .transpose()?
        .unwrap_or_else(|| json!({}));
    Ok(ToolCall {
        id: required_string(value, "id")?.to_owned(),
        name: required_string(function, "name")?.to_owned(),
        args,
    })
}

fn parse_arguments(arguments: &str) -> Result<Value, ModelError> {
    serde_json::from_str(arguments).map_err(|source| ModelError::Failed(source.to_string()))
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, ModelError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ModelError::Failed(format!("missing string field {field}")))
}
