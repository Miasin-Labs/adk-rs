use serde_json::{Value, json};

use super::state::{DevUiState, now_seconds};
use super::tools::ToolObservation;
use super::types::RunAgentRequest;

pub(crate) const ROOT_NODE_PATH: &str = "hello_world_agent@1";

pub(crate) fn user_event(state: &DevUiState, invocation_id: &str, text: &str) -> Value {
    json!({
        "id": state.new_event_id(),
        "invocationId": invocation_id,
        "author": "user",
        "timestamp": now_seconds(),
        "content": { "role": "user", "parts": [{ "text": text }] },
        "actions": base_actions(),
        "nodeInfo": { "path": "__START__" }
    })
}

pub(crate) fn tool_call_event(
    state: &DevUiState,
    invocation_id: &str,
    observation: &ToolObservation,
) -> Value {
    json!({
        "id": state.new_event_id(),
        "invocationId": invocation_id,
        "author": "hello_world_agent",
        "timestamp": now_seconds(),
        "content": { "role": "model", "parts": [{ "functionCall": { "id": observation.call_id, "name": observation.name, "args": observation.args } }] },
        "actions": base_actions(),
        "nodeInfo": { "path": ROOT_NODE_PATH }
    })
}

pub(crate) fn tool_response_event(
    state: &DevUiState,
    invocation_id: &str,
    observation: &ToolObservation,
) -> Value {
    let mut actions = base_actions();
    actions["stateDelta"] = observation.state_delta.clone().unwrap_or_else(|| json!({}));
    json!({
        "id": state.new_event_id(),
        "invocationId": invocation_id,
        "author": "hello_world_agent",
        "timestamp": now_seconds(),
        "content": { "role": "user", "parts": [{ "functionResponse": { "id": observation.call_id, "name": observation.name, "response": observation.response } }] },
        "actions": actions,
        "nodeInfo": { "path": ROOT_NODE_PATH }
    })
}

pub(crate) fn agent_event(state: &DevUiState, invocation_id: &str, text: &str) -> Value {
    json!({
        "id": state.new_event_id(),
        "invocationId": invocation_id,
        "author": "hello_world_agent",
        "timestamp": now_seconds(),
        "content": { "role": "model", "parts": [{ "text": text }] },
        "actions": base_actions(),
        "nodeInfo": { "path": ROOT_NODE_PATH }
    })
}

pub(crate) fn approval_event(
    state: &DevUiState,
    invocation_id: &str,
    approval_id: &str,
    tool_name: &str,
    message: &str,
    args: Value,
) -> Value {
    let mut actions = base_actions();
    actions["requestedToolConfirmations"] = json!({
        approval_id: {
            "toolName": tool_name,
            "message": message,
            "args": args
        }
    });
    json!({
        "id": state.new_event_id(),
        "invocationId": invocation_id,
        "author": "hello_world_agent",
        "timestamp": now_seconds(),
        "content": { "role": "model", "parts": [{ "text": message }] },
        "actions": actions,
        "nodeInfo": { "path": ROOT_NODE_PATH }
    })
}

pub(crate) fn error_event(state: &DevUiState, invocation_id: &str, message: &str) -> Value {
    json!({
        "id": state.new_event_id(),
        "invocationId": invocation_id,
        "author": "hello_world_agent",
        "timestamp": now_seconds(),
        "content": { "role": "model", "parts": [{ "text": message }] },
        "actions": base_actions(),
        "nodeInfo": { "path": ROOT_NODE_PATH }
    })
}

pub(crate) fn request_message_text(request: &RunAgentRequest) -> String {
    request
        .new_message
        .as_ref()
        .and_then(|message| message.get("parts"))
        .and_then(Value::as_array)
        .and_then(|parts| parts.first())
        .and_then(|part| part.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("hello")
        .to_owned()
}

pub(crate) fn base_actions() -> Value {
    json!({
        "stateDelta": {},
        "artifactDelta": {},
        "requestedAuthConfigs": {},
        "requestedToolConfirmations": {}
    })
}
