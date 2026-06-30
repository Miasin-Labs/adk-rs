use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use adk_rs::{
    AgentBuilder,
    AgentName,
    EventActions,
    FinishReason,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    ResumeDecision,
    RunConfig,
    Runner,
    SessionId,
    Tool,
    ToolApprovalPolicy,
    ToolCall,
    ToolError,
    ToolResult,
    ToolSpec,
};
use async_trait::async_trait;
use serde_json::json;

struct SendModel;

#[async_trait]
impl LanguageModel for SendModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "send-1".to_owned(),
                name: "send_email".to_owned(),
                args: json!({ "body": "trail recommendation" }),
            }],
            actions: EventActions::default(),
        })
    }
}

struct SendEmailTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for SendEmailTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "send_email".to_owned(),
            description: "Send email".to_owned(),
            input_schema: json!({ "type": "object" }),
        }
    }

    fn approval_policy(&self) -> ToolApprovalPolicy {
        ToolApprovalPolicy::Required {
            message: "Send outbound email?".to_owned(),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "sent": true }),
        })
    }
}

#[tokio::test]
async fn approval_required_tool_suspends_before_execution_normal() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = approval_runner(Arc::clone(&calls));

    let output = runner
        .run(
            &SessionId::new("approval-session").unwrap(),
            InvocationId::new("turn-1").unwrap(),
            "send it",
        )
        .await
        .unwrap();

    assert_eq!(output.finish_reason, FinishReason::Suspended);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(output.pending_approval.unwrap().tool_name, "send_email");
}

#[tokio::test]
async fn approving_pending_tool_executes_and_returns_result_normal() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = approval_runner(Arc::clone(&calls));
    let output = runner
        .run(
            &SessionId::new("approval-session").unwrap(),
            InvocationId::new("turn-1").unwrap(),
            "send it",
        )
        .await
        .unwrap();
    let pending = output.pending_approval.unwrap();

    let resumed = runner
        .resume_tool_call(
            &SessionId::new("approval-session").unwrap(),
            &pending.tool_call_id,
            ResumeDecision::Approved,
        )
        .await
        .unwrap();

    assert_eq!(resumed.finish_reason, FinishReason::Stop);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(resumed.events.iter().any(|event| {
        event
            .parts
            .iter()
            .any(|part| matches!(part, adk_rs::EventPart::ToolResult(result) if result.content["sent"] == true))
    }));
}

#[tokio::test]
async fn declining_pending_tool_returns_declined_result_normal() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = approval_runner(Arc::clone(&calls));
    let output = runner
        .run(
            &SessionId::new("approval-session").unwrap(),
            InvocationId::new("turn-1").unwrap(),
            "send it",
        )
        .await
        .unwrap();
    let pending = output.pending_approval.unwrap();

    let resumed = runner
        .resume_tool_call(
            &SessionId::new("approval-session").unwrap(),
            &pending.tool_call_id,
            ResumeDecision::Declined,
        )
        .await
        .unwrap();

    assert_eq!(resumed.finish_reason, FinishReason::Stop);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(resumed.events.iter().any(|event| {
        event
            .parts
            .iter()
            .any(|part| matches!(part, adk_rs::EventPart::ToolResult(result) if result.content["approved"] == false))
    }));
}

fn approval_runner(calls: Arc<AtomicUsize>) -> Runner<InMemorySessionStore> {
    let agent = AgentBuilder::new(
        AgentName::new("approval_agent").unwrap(),
        "send when approved",
        Arc::new(SendModel),
    )
    .tool(Arc::new(SendEmailTool { calls }))
    .build()
    .unwrap();
    Runner::new(InMemorySessionStore::default(), agent).with_run_config(RunConfig {
        max_iterations: Some(4),
        ..RunConfig::default()
    })
}
