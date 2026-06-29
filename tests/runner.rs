use std::sync::{Arc, Mutex};

use adk_rs::{
    AgentBuilder,
    AgentName,
    App,
    AppName,
    EventActions,
    EventAuthor,
    EventPart,
    InMemorySessionStore,
    InvocationContext,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    Plugin,
    PluginError,
    Runner,
    SessionId,
    Tool,
    ToolCall,
    ToolError,
    ToolResult,
    ToolSpec,
};
use async_trait::async_trait;
use serde_json::json;

struct ScriptedModel;

#[async_trait]
impl LanguageModel for ScriptedModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        assert_eq!(request.tools.len(), 1);
        Ok(ModelResponse {
            text: Some("done".to_owned()),
            tool_calls: vec![ToolCall {
                id: "call-1".to_owned(),
                name: "lookup".to_owned(),
                args: json!({ "query": "rust" }),
            }],
            actions: EventActions::default(),
        })
    }
}

struct ToolThenTextModel {
    calls: Mutex<usize>,
}

#[async_trait]
impl LanguageModel for ToolThenTextModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            assert_eq!(request.tools.len(), 1);
            return Ok(ModelResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "call-1".to_owned(),
                    name: "lookup".to_owned(),
                    args: json!({ "query": "rust" }),
                }],
                actions: EventActions::default(),
            });
        }

        assert!(
            request
                .events
                .iter()
                .any(|event| matches!(event.parts.first(), Some(EventPart::ToolResult(_))))
        );
        Ok(ModelResponse {
            text: Some("done after tool".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

struct TransferModel;

#[async_trait]
impl LanguageModel for TransferModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(format!("transfer from {}", request.instruction)),
            tool_calls: Vec::new(),
            actions: EventActions {
                transfer_to_agent: Some(AgentName::new("specialist").unwrap()),
                ..EventActions::default()
            },
        })
    }
}

struct SpecialistModel;

#[async_trait]
impl LanguageModel for SpecialistModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some("specialist answer".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

struct PanicModel;

#[async_trait]
impl LanguageModel for PanicModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Err(ModelError::Failed(
            "model should have been short-circuited".to_owned(),
        ))
    }
}

struct ShortCircuitPlugin;

#[async_trait]
impl Plugin for ShortCircuitPlugin {
    fn name(&self) -> &str {
        "short-circuit"
    }

    async fn before_model(
        &self,
        _context: &InvocationContext,
        _request: &ModelRequest,
    ) -> Result<Option<ModelResponse>, PluginError> {
        Ok(Some(ModelResponse {
            text: Some("plugin response".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        }))
    }
}

struct LookupTool;

#[async_trait]
impl Tool for LookupTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "lookup".to_owned(),
            description: "lookup data".to_owned(),
            input_schema: json!({ "type": "object" }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "answer": call.args["query"] }),
        })
    }
}

#[tokio::test]
async fn runner_records_user_tool_and_agent_events_normal() {
    let agent = AgentBuilder::new(
        AgentName::new("root").unwrap(),
        "answer with tools",
        Arc::new(ScriptedModel),
    )
    .tool(Arc::new(LookupTool))
    .build()
    .unwrap();
    let runner = Runner::new(InMemorySessionStore::default(), agent);

    let output = runner
        .run(
            &SessionId::new("s1").unwrap(),
            InvocationId::new("i1").unwrap(),
            "hello",
        )
        .await
        .unwrap();
    let events = output.events;

    assert!(matches!(events[0].author, EventAuthor::User));
    assert!(matches!(events[1].parts[0], EventPart::ToolResult(_)));
    assert!(matches!(events[2].author, EventAuthor::Agent(_)));
}

#[tokio::test]
async fn runner_feeds_tool_results_back_to_model_normal() {
    let agent = AgentBuilder::new(
        AgentName::new("root").unwrap(),
        "answer with tools",
        Arc::new(ToolThenTextModel {
            calls: Mutex::new(0),
        }),
    )
    .tool(Arc::new(LookupTool))
    .build()
    .unwrap();
    let runner = Runner::new(InMemorySessionStore::default(), agent);

    let output = runner
        .run(
            &SessionId::new("s2").unwrap(),
            InvocationId::new("i2").unwrap(),
            "hello",
        )
        .await
        .unwrap();

    assert!(matches!(
        output.events[1].parts[0],
        EventPart::ToolResult(_)
    ));
    assert!(matches!(output.events[2].author, EventAuthor::Agent(_)));
}

#[tokio::test]
async fn runner_transfers_to_agent_tree_member_normal() {
    let specialist = AgentBuilder::new(
        AgentName::new("specialist").unwrap(),
        "specialist",
        Arc::new(SpecialistModel),
    )
    .build()
    .unwrap();
    let root = AgentBuilder::new(
        AgentName::new("root").unwrap(),
        "root",
        Arc::new(TransferModel),
    )
    .sub_agent(specialist)
    .build()
    .unwrap();
    let runner = Runner::new(InMemorySessionStore::default(), root);

    let output = runner
        .run(
            &SessionId::new("s3").unwrap(),
            InvocationId::new("i3").unwrap(),
            "handoff",
        )
        .await
        .unwrap();

    assert_eq!(
        output.transfer_to_agent,
        Some(AgentName::new("specialist").unwrap())
    );
    assert!(output.events.iter().any(|event| {
        matches!(&event.author, EventAuthor::Agent(name) if name == &AgentName::new("specialist").unwrap())
    }));
}

#[tokio::test]
async fn runner_allows_plugin_to_short_circuit_model_normal() {
    let agent = AgentBuilder::new(
        AgentName::new("root").unwrap(),
        "root",
        Arc::new(PanicModel),
    )
    .build()
    .unwrap();
    let app = App::new(AppName::new("app").unwrap(), agent).plugin(Arc::new(ShortCircuitPlugin));
    let runner = Runner::from_app(InMemorySessionStore::default(), app);

    let output = runner
        .run(
            &SessionId::new("s4").unwrap(),
            InvocationId::new("i4").unwrap(),
            "hello",
        )
        .await
        .unwrap();

    assert!(matches!(
        &output.events[1].parts[0],
        EventPart::Text(text) if text == "plugin response"
    ));
}
