//! A parallel workflow: a `Parallel` agent fans its sub-agents out as isolated,
//! concurrent branches. Each branch sees the same snapshot of the session taken
//! at fan-out (no branch observes another branch's output), the branches run
//! concurrently, then their results merge back into the session in declaration
//! order. This is the fan-out/fan-in shape from the agent-architecture
//! literature (e.g. a risk assessment scored across independent dimensions).
//!
//! Run with: `cargo run --example parallel_workflow`
//!
//! The models here are scripted so the example runs with no API key. Swap them
//! for a real `LanguageModel` adapter to drive the same fan-out against a live
//! provider — there the concurrency is a real wall-clock win for I/O-bound
//! model calls.

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

/// One independent risk dimension. It reports how many events it saw so the
/// output makes the isolation visible: every branch sees the same fan-out
/// snapshot, never a sibling's result.
struct RiskModel {
    finding: &'static str,
}

#[async_trait]
impl LanguageModel for RiskModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(format!(
                "{} (scored against {} shared events)",
                self.finding,
                request.events.len()
            )),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dimensions = [
        ("credit_risk", "credit: debt-to-income 0.28, low default probability"),
        ("market_risk", "market: moderate rate sensitivity, contained sector exposure"),
        ("operational_risk", "operational: no fraud indicators, capacity adequate"),
        ("compliance_risk", "compliance: KYC/AML clear, no jurisdictional flags"),
    ];

    let mut builder = AgentBuilder::new(
        AgentName::new("risk_assessment")?,
        "Assess the application across independent risk dimensions.",
        // The parent's own model is never called; it only fans out.
        Arc::new(RiskModel { finding: "unused root" }) as Arc<dyn LanguageModel>,
    )
    .parallel();

    for (name, finding) in dimensions {
        let branch = AgentBuilder::new(
            AgentName::new(name)?,
            "Score one risk dimension.",
            Arc::new(RiskModel { finding }) as Arc<dyn LanguageModel>,
        )
        .build()?;
        builder = builder.sub_agent(branch);
    }
    let assessment = builder.build()?;

    let runner = Runner::new(InMemorySessionStore::default(), assessment);
    let output = runner
        .run(
            &SessionId::new("parallel-demo")?,
            InvocationId::new("turn-1")?,
            "Evaluate this loan application for risk.",
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

    // Every branch reports the same shared-event count: proof they ran in
    // isolation rather than chaining off one another.
    Ok(())
}
