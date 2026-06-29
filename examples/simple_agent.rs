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

struct PlannerModel;

#[async_trait]
impl LanguageModel for PlannerModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let user_text = request
            .events
            .iter()
            .rev()
            .flat_map(|event| event.parts.iter())
            .find_map(|part| match part {
                EventPart::Text(text) => Some(text.as_str()),
                EventPart::ToolCall(_) | EventPart::ToolResult(_) => None,
            })
            .unwrap_or("no user request");

        Ok(ModelResponse {
            text: Some(format!(
                "Plan for '{user_text}': clarify the goal, make the smallest useful draft, then verify it."
            )),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model: Arc<dyn LanguageModel> = Arc::new(PlannerModel);
    let agent = AgentBuilder::new(
        AgentName::new("planner")?,
        "Turn a user request into a short execution plan.",
        model,
    )
    .description("A tiny planning agent")
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), agent);
    let output = runner
        .run(
            &SessionId::new("simple-demo")?,
            InvocationId::new("turn-1")?,
            "Write docs for the crate",
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
