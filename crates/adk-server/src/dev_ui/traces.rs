use serde_json::{Value, json};

use super::state::{DevUiState, now_seconds};

pub async fn event_trace(state: &DevUiState, event_id: &str) -> Value {
    match state.event_by_id(event_id).await {
        Some(event) => json!({ "event": event, "spans": event_spans("adhoc", &[event]) }),
        None => json!({ "event": null, "spans": [] }),
    }
}

pub async fn session_trace(state: &DevUiState, session_id: &str) -> Vec<Value> {
    let events = state.session_events(session_id).await;
    event_spans(session_id, &events)
}

fn event_spans(session_id: &str, events: &[Value]) -> Vec<Value> {
    let mut spans = vec![root_span(session_id)];
    spans.extend(
        events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| event_span(session_id, index, event)),
    );
    spans
}

fn root_span(session_id: &str) -> Value {
    let start = now_ns();
    json!({
        "name": "invoke_agent hello_world_agent",
        "span_id": format!("span-root-{session_id}"),
        "trace_id": format!("trace-{session_id}"),
        "parent_span_id": null,
        "start_time": start,
        "end_time": start + 1_000_000,
        "attributes": { "gen_ai.operation.name": "invoke_agent", "gen_ai.conversation.id": session_id, "gen_ai.agent.name": "hello_world_agent" }
    })
}

fn event_span(session_id: &str, index: usize, event: &Value) -> Option<Value> {
    let event_id = event.get("id")?.as_str()?;
    let invocation_id = event
        .get("invocationId")
        .and_then(Value::as_str)
        .unwrap_or("invocation");
    let author = event
        .get("author")
        .and_then(Value::as_str)
        .unwrap_or("hello_world_agent");
    let start = now_ns() + u64::try_from(index).ok()? * 2_000_000;
    Some(json!({
        "name": span_name(event),
        "span_id": format!("span-{event_id}"),
        "trace_id": format!("trace-{session_id}"),
        "parent_span_id": format!("span-root-{session_id}"),
        "start_time": start,
        "end_time": start + 1_000_000,
        "attributes": {
            "gen_ai.operation.name": "generate_content",
            "gen_ai.agent.name": author,
            "gcp.vertex.agent.event_id": event_id,
            "gcp.vertex.agent.invocation_id": invocation_id,
            "gcp.vertex.agent.associated_event_ids": [event_id],
            "gcp.vertex.agent.llm_request": event.to_string(),
            "gcp.vertex.agent.llm_response": event.to_string()
        }
    }))
}

fn span_name(event: &Value) -> &'static str {
    let part = event
        .get("content")
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .and_then(|parts| parts.first());
    if part.and_then(|part| part.get("functionCall")).is_some() {
        "call_tool"
    } else if part.and_then(|part| part.get("functionResponse")).is_some() {
        "tool_response"
    } else {
        "generate_content hello_world_agent"
    }
}

fn now_ns() -> u64 {
    (now_seconds() * 1_000_000_000.0) as u64
}
