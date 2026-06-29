use std::sync::Arc;

use adk_rs::{
    AgentBuilder,
    AgentName,
    EventActions,
    EventAuthor,
    EventPart,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    Runner,
    SessionId,
};
use async_trait::async_trait;

struct RouterModel {
    target: AgentName,
}

#[async_trait]
impl LanguageModel for RouterModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some("This should go to the support specialist.".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions {
                transfer_to_agent: Some(self.target.clone()),
                ..EventActions::default()
            },
        })
    }
}

struct SpecialistModel;

#[async_trait]
impl LanguageModel for SpecialistModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(format!(
                "Specialist answer after seeing {} prior events: keep the fix small and verify it.",
                request.events.len()
            )),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let specialist_name = AgentName::new("support_specialist")?;
    let specialist_model: Arc<dyn LanguageModel> = Arc::new(SpecialistModel);
    let specialist = AgentBuilder::new(
        specialist_name.clone(),
        "Resolve implementation details after the router hands off.",
        specialist_model,
    )
    .build()?;

    let router_model: Arc<dyn LanguageModel> = Arc::new(RouterModel {
        target: specialist_name,
    });
    let router = AgentBuilder::new(
        AgentName::new("router")?,
        "Route requests to the right specialist.",
        router_model,
    )
    .sub_agent(specialist)
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), router);
    let output = runner
        .run(
            &SessionId::new("handoff-demo")?,
            InvocationId::new("turn-1")?,
            "Please review this implementation path.",
        )
        .await?;

    for event in output.events {
        let author = match event.author {
            EventAuthor::User => "user".to_owned(),
            EventAuthor::Agent(name) => format!("agent:{}", name.as_str()),
            EventAuthor::Tool(name) => format!("tool:{name}"),
        };

        for part in event.parts {
            if let EventPart::Text(text) = part {
                println!("{author}: {text}");
            }
        }
    }

    Ok(())
}
