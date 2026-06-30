use std::sync::{Arc, Mutex};

use adk_rs::{
    AgentBuilder,
    AgentName,
    AgentPrompt,
    EventActions,
    EventAuthor,
    EventPart,
    FinishReason,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    RunConfig,
    RunTraceStep,
    Runner,
    Session,
    SessionId,
    SessionStore,
    Tool,
    ToolCall,
    ToolError,
    ToolResult,
    ToolSpec,
};
use async_trait::async_trait;
use serde_json::json;

struct LoopingToolModel;

#[async_trait]
impl LanguageModel for LoopingToolModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "loop-call".to_owned(),
                name: "noop".to_owned(),
                args: json!({}),
            }],
            actions: EventActions::default(),
        })
    }
}

struct CapturingModel {
    event_counts: Arc<Mutex<Vec<usize>>>,
    first_texts: Arc<Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl LanguageModel for CapturingModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        self.event_counts.lock().unwrap().push(request.events.len());
        let first_text = request.events.first().and_then(|event| {
            event.parts.iter().find_map(|part| match part {
                EventPart::Text(text) => Some(text.clone()),
                EventPart::ToolCall(_) | EventPart::ToolResult(_) => None,
            })
        });
        self.first_texts.lock().unwrap().push(first_text);
        Ok(ModelResponse {
            text: Some("captured".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

struct NoopTool;

#[async_trait]
impl Tool for NoopTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "noop".to_owned(),
            description: "no-op tool".to_owned(),
            input_schema: json!({ "type": "object" }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "ok": true }),
        })
    }
}

#[test]
fn agent_prompt_renders_beginner_agent_contract_normal() {
    let prompt = AgentPrompt::new("Trail advisor")
        .task("Recommend one trail run for the morning")
        .input("Calendar events, weather, air quality, and saved trails")
        .tools(["calendar_read", "weather_get", "trail_list", "send_email"])
        .constraints([
            "Do not send email without approval",
            "Prefer safer air quality",
        ])
        .output("A concise recommendation with the reason");

    let rendered = prompt.render();

    assert!(rendered.contains("Role: Trail advisor"));
    assert!(rendered.contains("Task: Recommend one trail run for the morning"));
    assert!(rendered.contains("- calendar_read"));
    assert!(rendered.contains("- Do not send email without approval"));
    assert!(rendered.contains("Output: A concise recommendation with the reason"));
}

#[tokio::test]
async fn runner_stops_with_max_iterations_and_records_trace_normal() {
    let agent = AgentBuilder::new(
        AgentName::new("root").unwrap(),
        "keep calling tools",
        Arc::new(LoopingToolModel),
    )
    .tool(Arc::new(NoopTool))
    .build()
    .unwrap();
    let runner = Runner::new(InMemorySessionStore::default(), agent).with_run_config(RunConfig {
        max_iterations: Some(1),
        ..RunConfig::default()
    });

    let output = runner
        .run(
            &SessionId::new("max-iterations").unwrap(),
            InvocationId::new("turn-1").unwrap(),
            "start",
        )
        .await
        .unwrap();

    assert_eq!(output.finish_reason, FinishReason::MaxIterations);
    assert!(output.trace.steps.iter().any(|step| {
        matches!(step, RunTraceStep::ModelCall { agent_name, .. } if agent_name.as_str() == "root")
    }));
    assert!(output.trace.steps.iter().any(|step| {
        matches!(step, RunTraceStep::ToolCall { tool_name, call_id } if tool_name == "noop" && call_id == "loop-call")
    }));
}

#[tokio::test]
async fn runner_limits_model_request_to_memory_window_normal() {
    let event_counts = Arc::new(Mutex::new(Vec::new()));
    let first_texts = Arc::new(Mutex::new(Vec::new()));
    let store = InMemorySessionStore::default();
    let session_id = SessionId::new("memory-window").unwrap();
    let mut session = Session::new(session_id.clone());
    session.append(EventPart::Text("old-one".to_owned()).into_event("old-1"));
    session.append(EventPart::Text("old-two".to_owned()).into_event("old-2"));
    store.create(session).unwrap();

    let agent = AgentBuilder::new(
        AgentName::new("root").unwrap(),
        "capture request",
        Arc::new(CapturingModel {
            event_counts: Arc::clone(&event_counts),
            first_texts: Arc::clone(&first_texts),
        }),
    )
    .build()
    .unwrap();
    let runner = Runner::new(store, agent).with_run_config(RunConfig {
        memory_window_events: Some(2),
        ..RunConfig::default()
    });

    runner
        .run(&session_id, InvocationId::new("turn-1").unwrap(), "new")
        .await
        .unwrap();

    assert_eq!(*event_counts.lock().unwrap(), vec![2]);
    assert_eq!(
        *first_texts.lock().unwrap(),
        vec![Some("old-two".to_owned())]
    );
}

trait TextEventExt {
    fn into_event(self, invocation_id: &str) -> adk_rs::Event;
}

impl TextEventExt for EventPart {
    fn into_event(self, invocation_id: &str) -> adk_rs::Event {
        adk_rs::Event {
            id: adk_rs::EventId::for_index(0),
            invocation_id: InvocationId::new(invocation_id).unwrap(),
            author: EventAuthor::User,
            parts: vec![self],
            actions: EventActions::default(),
            timestamp_seconds: 0,
        }
    }
}
