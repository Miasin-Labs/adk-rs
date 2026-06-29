use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use super::config::OpenAiConfig;
use super::tools::{self, ToolObservation};

const INSTRUCTION: &str = "You are hello_world_agent. You roll dice only by calling roll_die. You check primes only by calling check_prime. When asked to roll and check a die, call roll_die first, then call check_prime with the rolled result, then answer with the roll result and prime status.";

#[derive(Clone)]
pub struct OpenAiAgent {
    client: reqwest::Client,
    config: OpenAiConfig,
}

pub struct AgentRun {
    pub text: String,
    pub tools: Vec<ToolObservation>,
}

impl OpenAiAgent {
    pub fn load() -> Option<Self> {
        let config = OpenAiConfig::load()?;
        Some(Self {
            client: reqwest::Client::new(),
            config,
        })
    }

    /// General single-turn completion with a caller-supplied system
    /// instruction and no built-in tools. Used by the n8n "ADK Agent" node,
    /// which carries its own instructions/prompt rather than the dice demo's.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": user }),
        ];
        let message = self.chat_no_tools(&messages).await?;
        Ok(message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("OpenAI returned an empty response.")
            .to_owned())
    }

    async fn chat_no_tools(&self, messages: &[Value]) -> Result<Value, String> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .headers(headers(&self.config.api_key)?)
            .json(&json!({ "model": self.config.model, "messages": messages }))
            .send()
            .await
            .map_err(|error| format!("OpenAI request failed: {error}"))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|error| format!("OpenAI response parse failed: {error}"))?;
        if !status.is_success() {
            return Err(openai_error(status.as_u16(), &body));
        }
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
            .ok_or_else(|| "OpenAI response did not include a message".to_owned())
    }

    /// Run an agent turn with a caller-selected set of tools (used by the n8n
    /// "ADK Agent" node's `tools` parameter). With no tools this degrades to a
    /// plain completion.
    pub async fn run_with_tools(
        &self,
        system: &str,
        user: &str,
        tool_names: &[String],
    ) -> Result<AgentRun, String> {
        let definitions = selected_tool_definitions(tool_names);
        if definitions.as_array().is_none_or(|array| array.is_empty()) {
            let text = self.complete(system, user).await?;
            return Ok(AgentRun { text, tools: Vec::new() });
        }
        let mut messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": user }),
        ];
        let mut rolls = Vec::new();
        let mut observations = Vec::new();
        for _ in 0..6 {
            let message = self.chat_with(&messages, &definitions).await?;
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                messages.push(message.clone());
                for call in tool_calls {
                    let observation = execute_general_tool_call(call, &mut rolls).await;
                    messages.push(tool_message(&observation));
                    observations.push(observation);
                }
                continue;
            }
            let text = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("OpenAI returned an empty response.")
                .to_owned();
            return Ok(AgentRun { text, tools: observations });
        }
        Err("Agent tool loop did not finish in 6 turns".to_owned())
    }

    /// Chat completion with a caller-supplied tool set.
    async fn chat_with(&self, messages: &[Value], tools: &Value) -> Result<Value, String> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .headers(headers(&self.config.api_key)?)
            .json(&json!({
                "model": self.config.model,
                "messages": messages,
                "tools": tools,
                "tool_choice": "auto",
            }))
            .send()
            .await
            .map_err(|error| format!("OpenAI request failed: {error}"))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|error| format!("OpenAI response parse failed: {error}"))?;
        if !status.is_success() {
            return Err(openai_error(status.as_u16(), &body));
        }
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
            .ok_or_else(|| "OpenAI response did not include a message".to_owned())
    }

    pub async fn run(&self, prompt: &str, rolls: &mut Vec<i64>) -> Result<AgentRun, String> {
        let mut messages = vec![
            json!({ "role": "system", "content": INSTRUCTION }),
            json!({ "role": "user", "content": prompt }),
        ];
        let mut observations = Vec::new();
        for _ in 0..4 {
            let message = self.chat(&messages).await?;
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                messages.push(message.clone());
                for call in tool_calls {
                    let observation = execute_tool_call(call, rolls);
                    messages.push(tool_message(&observation));
                    observations.push(observation);
                }
                continue;
            }
            let text = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("OpenAI returned an empty response.")
                .to_owned();
            return Ok(AgentRun {
                text,
                tools: observations,
            });
        }
        Err("OpenAI tool loop did not finish in 4 turns".to_owned())
    }

    async fn chat(&self, messages: &[Value]) -> Result<Value, String> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .headers(headers(&self.config.api_key)?)
            .json(&json!({
                "model": self.config.model,
                "messages": messages,
                "tools": tool_definitions(),
                "tool_choice": "auto",
            }))
            .send()
            .await
            .map_err(|error| format!("OpenAI request failed: {error}"))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|error| format!("OpenAI response parse failed: {error}"))?;
        if !status.is_success() {
            return Err(openai_error(status.as_u16(), &body));
        }
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
            .ok_or_else(|| "OpenAI response did not include a message".to_owned())
    }
}

fn execute_tool_call(call: &Value, rolls: &mut Vec<i64>) -> ToolObservation {
    let call_id = call
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("tool-call")
        .to_owned();
    let function = call.get("function").unwrap_or(&Value::Null);
    let name = function.get("name").and_then(Value::as_str).unwrap_or("");
    let args = function
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|args| serde_json::from_str::<Value>(args).ok())
        .unwrap_or_else(|| json!({}));
    tools::execute(call_id, name, args, rolls)
}

/// OpenAI tool schemas for the agent-node-selectable tools, filtered by name.
fn selected_tool_definitions(tool_names: &[String]) -> Value {
    let all = json!([
        { "type": "function", "function": { "name": "roll_die", "description": "Roll a die and return the rolled result.", "parameters": { "type": "object", "properties": { "sides": { "type": "integer", "minimum": 1 } }, "required": ["sides"] } } },
        { "type": "function", "function": { "name": "check_prime", "description": "Check whether integers are prime.", "parameters": { "type": "object", "properties": { "nums": { "type": "array", "items": { "type": "integer" } } }, "required": ["nums"] } } },
        { "type": "function", "function": { "name": "httpRequest", "description": "Make an HTTP request and return the response body.", "parameters": { "type": "object", "properties": { "method": { "type": "string", "enum": ["GET", "POST", "PUT", "DELETE"] }, "url": { "type": "string" }, "body": { "type": "object" } }, "required": ["url"] } } },
        { "type": "function", "function": { "name": "get_time", "description": "Return the current date and time (UTC, ISO-8601).", "parameters": { "type": "object", "properties": {} } } },
        { "type": "function", "function": { "name": "calculator", "description": "Evaluate a numeric/JS math expression and return the result.", "parameters": { "type": "object", "properties": { "expression": { "type": "string" } }, "required": ["expression"] } } }
    ]);
    let selected: Vec<Value> = all
        .as_array()
        .into_iter()
        .flatten()
        .filter(|definition| {
            definition
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .is_some_and(|name| tool_names.iter().any(|selected| selected == name))
        })
        .cloned()
        .collect();
    Value::Array(selected)
}

/// Execute a tool call from the general agent loop (async for `httpRequest`).
async fn execute_general_tool_call(call: &Value, rolls: &mut Vec<i64>) -> ToolObservation {
    let call_id = call
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("tool-call")
        .to_owned();
    let function = call.get("function").unwrap_or(&Value::Null);
    let name = function.get("name").and_then(Value::as_str).unwrap_or("");
    let args = function
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|args| serde_json::from_str::<Value>(args).ok())
        .unwrap_or_else(|| json!({}));
    match name {
        "httpRequest" => {
            let method = args.get("method").and_then(Value::as_str).unwrap_or("GET");
            let url = args.get("url").and_then(Value::as_str).unwrap_or("");
            let response = tools::http_request(method, url, args.get("body"), &[]).await;
            ToolObservation { call_id, name: name.to_owned(), args, response, state_delta: None }
        }
        "get_time" => {
            let response = json!({ "iso": tools::now_iso(), "unixMs": tools::now_unix_ms() });
            ToolObservation { call_id, name: name.to_owned(), args, response, state_delta: None }
        }
        "calculator" => {
            let expression = args.get("expression").and_then(Value::as_str).unwrap_or("");
            let response = match super::n8n::expr::eval_math(expression) {
                Ok(result) => json!({ "result": result }),
                Err(error) => json!({ "error": error }),
            };
            ToolObservation { call_id, name: name.to_owned(), args, response, state_delta: None }
        }
        _ => tools::execute(call_id, name, args, rolls),
    }
}

fn tool_message(observation: &ToolObservation) -> Value {
    json!({
        "role": "tool",
        "tool_call_id": observation.call_id,
        "content": observation.response.to_string(),
    })
}

fn headers(api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let value = HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|_| "OPENAI_API_KEY contains invalid header characters".to_owned())?;
    headers.insert(AUTHORIZATION, value);
    Ok(headers)
}

fn tool_definitions() -> Value {
    json!([
        { "type": "function", "function": { "name": "roll_die", "description": "Roll a die and return the rolled result.", "parameters": { "type": "object", "properties": { "sides": { "type": "integer", "minimum": 1 } }, "required": ["sides"] } } },
        { "type": "function", "function": { "name": "check_prime", "description": "Check whether integers are prime.", "parameters": { "type": "object", "properties": { "nums": { "type": "array", "items": { "type": "integer" } } }, "required": ["nums"] } } }
    ])
}

fn openai_error(status: u16, body: &Value) -> String {
    let message = body
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown OpenAI error");
    format!("OpenAI returned HTTP {status}: {message}")
}
