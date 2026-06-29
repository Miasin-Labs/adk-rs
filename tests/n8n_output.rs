use std::sync::Arc;

use adk_rs::{
    AgentBuilder,
    AgentName,
    EventActions,
    FallbackLanguageModel,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    RunConfig,
    Runner,
    SessionId,
    StructuredOutputSchema,
};
use async_trait::async_trait;
use serde_json::json;

struct JsonModel;

#[async_trait]
impl LanguageModel for JsonModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(r#"{"trail":"Corner Canyon","send":false}"#.to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

struct FailingModel;

#[async_trait]
impl LanguageModel for FailingModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Err(ModelError::Failed("primary unavailable".to_owned()))
    }
}

struct BackupModel;

#[async_trait]
impl LanguageModel for BackupModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some("backup response".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

#[tokio::test]
async fn runner_parses_structured_output_when_schema_configured_normal() {
    let agent = AgentBuilder::new(
        AgentName::new("structured").unwrap(),
        "Return JSON.",
        Arc::new(JsonModel),
    )
    .build()
    .unwrap();
    let runner = Runner::new(InMemorySessionStore::default(), agent).with_run_config(RunConfig {
        structured_output_schema: Some(StructuredOutputSchema::new(json!({
            "type": "object",
            "required": ["trail", "send"]
        }))),
        ..RunConfig::default()
    });

    let output = runner
        .run(
            &SessionId::new("structured").unwrap(),
            InvocationId::new("turn-1").unwrap(),
            "pick a trail",
        )
        .await
        .unwrap();

    assert_eq!(
        output.structured_output,
        Some(json!({ "trail": "Corner Canyon", "send": false }))
    );
}

#[tokio::test]
async fn fallback_language_model_uses_backup_after_primary_failure_normal() {
    let primary: Arc<dyn LanguageModel> = Arc::new(FailingModel);
    let backup: Arc<dyn LanguageModel> = Arc::new(BackupModel);
    let model = FallbackLanguageModel::new(vec![primary, backup]);
    let response = model
        .generate(ModelRequest {
            instruction: "answer".to_owned(),
            events: Vec::new(),
            tools: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(response.text, Some("backup response".to_owned()));
}
