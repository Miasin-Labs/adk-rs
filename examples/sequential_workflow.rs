//! A sequential workflow: a `Sequential` agent runs its sub-agents in
//! declaration order over one shared session, so each stage builds on the
//! previous stage's output. This mirrors the draft -> review -> polish shape
//! from the agent-architecture literature.
//!
//! Run with: `cargo run --example sequential_workflow`
//!
//! The models here are scripted so the example runs with no API key. Swap them
//! for a real `LanguageModel` adapter (e.g. `OpenAiCompatibleModel`) to drive
//! the same pipeline against a live provider.

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

/// A stage model that emits a fixed line. Because every stage shares the
/// session, `request.events` grows as the pipeline advances — each stage can
/// see everything the earlier stages produced.
struct StageModel {
    line: &'static str,
}

#[async_trait]
impl LanguageModel for StageModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(format!("{} (after {} prior events)", self.line, request.events.len())),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Stage 1: scope the request.
    let scope = AgentBuilder::new(
        AgentName::new("scope")?,
        "Determine what analysis the request needs.",
        Arc::new(StageModel {
            line: "scope: this is a descriptive sales-by-region analysis",
        }) as Arc<dyn LanguageModel>,
    )
    .build()?;

    // Stage 2: do the analysis, building on the scoping output.
    let analyze = AgentBuilder::new(
        AgentName::new("analyze")?,
        "Run the analysis the scope stage identified.",
        Arc::new(StageModel {
            line: "analyze: Q4 revenue up 12%, strongest in the West region",
        }) as Arc<dyn LanguageModel>,
    )
    .build()?;

    // Stage 3: summarize for the stakeholder.
    let report = AgentBuilder::new(
        AgentName::new("report")?,
        "Summarize the analysis for a stakeholder.",
        Arc::new(StageModel {
            line: "report: West led Q4 (+12%); recommend doubling its ad budget",
        }) as Arc<dyn LanguageModel>,
    )
    .build()?;

    // The `Sequential` root runs scope -> analyze -> report in order. Its own
    // model is never called; it only orchestrates the sub-agents.
    let pipeline = AgentBuilder::new(
        AgentName::new("insights_pipeline")?,
        "Run the data-insights stages in order.",
        Arc::new(StageModel { line: "unused root" }) as Arc<dyn LanguageModel>,
    )
    .sequential()
    .sub_agent(scope)
    .sub_agent(analyze)
    .sub_agent(report)
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), pipeline);
    let output = runner
        .run(
            &SessionId::new("seq-demo")?,
            InvocationId::new("turn-1")?,
            "Analyze Q4 sales performance by region.",
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
