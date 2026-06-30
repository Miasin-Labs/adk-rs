//! A loop workflow: a `Loop` agent re-runs its sub-agent pipeline until a child
//! signals completion with an `escalate` action, or `max_iterations` is hit.
//! This is the evaluator-optimizer / "refine until good enough" shape — here a
//! single reviser that keeps improving a draft and escalates once it is happy.
//!
//! Run with: `cargo run --example loop_workflow`
//!
//! The model is scripted so the example runs with no API key. Swap it for a real
//! `LanguageModel` adapter to drive the same loop against a live provider.

use std::sync::{Arc, Mutex};

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

/// A reviser that improves a draft each pass and escalates (stops the loop)
/// once its self-assessed quality clears the bar.
struct ReviserModel {
    pass: Mutex<u32>,
    good_enough_at: u32,
}

#[async_trait]
impl LanguageModel for ReviserModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut pass = self.pass.lock().unwrap();
        *pass += 1;
        let quality = *pass; // pretend each pass raises quality by one notch
        let done = quality >= self.good_enough_at;
        let text = if done {
            format!("pass {pass}: quality {quality}/{} — good enough, finalizing", self.good_enough_at)
        } else {
            format!("pass {pass}: quality {quality}/{} — revising again", self.good_enough_at)
        };
        Ok(ModelResponse {
            text: Some(text),
            tool_calls: Vec::new(),
            actions: EventActions {
                // The escalate signal is what breaks the loop.
                escalate: if done { Some(true) } else { None },
                ..EventActions::default()
            },
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reviser = AgentBuilder::new(
        AgentName::new("reviser")?,
        "Improve the draft; escalate when it is good enough.",
        Arc::new(ReviserModel {
            pass: Mutex::new(0),
            good_enough_at: 3,
        }) as Arc<dyn LanguageModel>,
    )
    .build()?;

    // The loop runs the reviser up to 10 times, but stops early as soon as the
    // reviser escalates. The parent's own model is never called.
    let refine = AgentBuilder::new(
        AgentName::new("refine_loop")?,
        "Refine until good enough.",
        Arc::new(ReviserModel {
            pass: Mutex::new(0),
            good_enough_at: 1,
        }) as Arc<dyn LanguageModel>,
    )
    .loop_agent(10)
    .sub_agent(reviser)
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), refine);
    let output = runner
        .run(
            &SessionId::new("loop-demo")?,
            InvocationId::new("turn-1")?,
            "Draft and refine the release announcement.",
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
    println!("finish reason: {:?}", output.finish_reason);

    Ok(())
}
