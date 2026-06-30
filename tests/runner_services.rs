//! Integration tests for services wired into the Runner: telemetry spans,
//! memory retrieval injection, skill injection, planner injection, post-turn
//! metrics, and the streaming API.

use std::sync::Arc;

use adk_rs::{
    AgentBuilder,
    AgentName,
    AppName,
    EventActions,
    ExactMatchEvaluator,
    InMemoryMemoryService,
    InMemorySessionStore,
    InMemoryTelemetrySink,
    InvocationContext,
    InvocationId,
    LanguageModel,
    MemoryEntry,
    MemoryService,
    ModelError,
    ModelRequest,
    ModelResponse,
    Plan,
    PlanStep,
    Planner,
    PlannerError,
    RunStreamItem,
    Runner,
    SessionId,
    Skill,
    SkillRegistry,
    TelemetrySink,
    UserId,
};
use async_trait::async_trait;
use futures::StreamExt;

/// Echoes the instruction it received so tests can assert what was injected.
struct EchoInstructionModel;

#[async_trait]
impl LanguageModel for EchoInstructionModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(request.instruction.clone()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

/// Emits a fixed answer regardless of input.
struct FixedModel {
    answer: &'static str,
}

#[async_trait]
impl LanguageModel for FixedModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            text: Some(self.answer.to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

fn echo_agent() -> adk_rs::Agent {
    AgentBuilder::new(
        AgentName::new("assistant").unwrap(),
        "Base instruction.",
        Arc::new(EchoInstructionModel),
    )
    .build()
    .unwrap()
}

#[tokio::test]
async fn telemetry_sink_records_spans_during_run_normal() {
    let sink = Arc::new(InMemoryTelemetrySink::default());
    let runner = Runner::new(InMemorySessionStore::default(), echo_agent())
        .telemetry(sink.clone() as Arc<dyn TelemetrySink>);

    runner
        .run(
            &SessionId::new("s").unwrap(),
            InvocationId::new("i").unwrap(),
            "hello",
        )
        .await
        .unwrap();

    let spans = sink.spans().unwrap();
    assert!(!spans.is_empty(), "a run must record at least one span");
    assert!(spans.iter().any(|span| span.name.starts_with("run:")));
}

#[tokio::test]
async fn memory_injection_surfaces_retrieved_text_to_model_normal() {
    let memory = Arc::new(InMemoryMemoryService::default());
    memory
        .add_memory(
            &AppName::new("default_app").unwrap(),
            &UserId::new("default_user").unwrap(),
            MemoryEntry {
                text: "The capybara's name is Cornelius.".to_owned(),
                metadata: Default::default(),
            },
        )
        .unwrap();

    let runner = Runner::new(InMemorySessionStore::default(), echo_agent())
        .memory(memory as Arc<dyn MemoryService>);

    let output = runner
        .run(
            &SessionId::new("s").unwrap(),
            InvocationId::new("i").unwrap(),
            "capybara",
        )
        .await
        .unwrap();

    // The echo model returns its instruction, which must contain the recalled
    // memory entry injected by retrieval.
    let final_text = last_agent_text(&output);
    assert!(
        final_text.contains("Cornelius"),
        "retrieved memory should be injected into the model instruction, got: {final_text}"
    );
}

#[tokio::test]
async fn skill_injection_surfaces_skill_prompt_to_model_normal() {
    let mut registry = SkillRegistry::default();
    registry.register(Skill {
        name: "summarize".to_owned(),
        description: "summarize text".to_owned(),
        prompt: "Always answer in exactly one sentence.".to_owned(),
    });

    let runner = Runner::new(InMemorySessionStore::default(), echo_agent())
        .skills(Arc::new(registry));

    let output = runner
        .run(
            &SessionId::new("s").unwrap(),
            InvocationId::new("i").unwrap(),
            "hi",
        )
        .await
        .unwrap();

    let final_text = last_agent_text(&output);
    assert!(final_text.contains("Always answer in exactly one sentence."));
    assert!(final_text.contains("Base instruction."));
}

struct FixedPlanner;

#[async_trait]
impl Planner for FixedPlanner {
    async fn build_plan(
        &self,
        _context: &InvocationContext,
        _task: &str,
    ) -> Result<Plan, PlannerError> {
        Ok(Plan {
            steps: vec![
                PlanStep {
                    id: "1".to_owned(),
                    description: "Gather the facts.".to_owned(),
                },
                PlanStep {
                    id: "2".to_owned(),
                    description: "Answer concisely.".to_owned(),
                },
            ],
        })
    }
}

#[tokio::test]
async fn planner_injects_plan_steps_into_model_normal() {
    let runner = Runner::new(InMemorySessionStore::default(), echo_agent())
        .planner(Arc::new(FixedPlanner));

    let output = runner
        .run(
            &SessionId::new("s").unwrap(),
            InvocationId::new("i").unwrap(),
            "do it",
        )
        .await
        .unwrap();

    let final_text = last_agent_text(&output);
    assert!(final_text.contains("Gather the facts."));
    assert!(final_text.contains("Answer concisely."));
}

#[tokio::test]
async fn metric_evaluation_runs_post_turn_normal() {
    let agent = AgentBuilder::new(
        AgentName::new("assistant").unwrap(),
        "answer",
        Arc::new(FixedModel { answer: "42" }),
    )
    .build()
    .unwrap();
    let runner = Runner::new(InMemorySessionStore::default(), agent)
        .metric(Arc::new(ExactMatchEvaluator));

    let output = runner
        .run(
            &SessionId::new("s").unwrap(),
            InvocationId::new("i").unwrap(),
            "q",
        )
        .await
        .unwrap();

    assert_eq!(output.metrics.len(), 1);
    assert_eq!(output.metrics[0].name, "exact_match");
}

#[tokio::test]
async fn stream_yields_events_then_done_normal() {
    let runner = Runner::new(InMemorySessionStore::default(), echo_agent());
    let mut stream = runner.stream(
        &SessionId::new("s").unwrap(),
        InvocationId::new("i").unwrap(),
        "hello",
    );

    let mut event_count = 0;
    let mut saw_done = false;
    while let Some(item) = stream.next().await {
        match item.unwrap() {
            RunStreamItem::Event(_) => {
                assert!(!saw_done, "events must arrive before the Done item");
                event_count += 1;
            }
            RunStreamItem::Done(output) => {
                saw_done = true;
                // Done carries the full event list too.
                assert!(!output.events.is_empty());
            }
        }
    }
    assert!(saw_done, "stream must end with a Done item");
    assert!(event_count >= 2, "expected at least user + agent events");
}

fn last_agent_text(output: &adk_rs::RunOutput) -> String {
    use adk_rs::{EventAuthor, EventPart};
    output
        .events
        .iter()
        .rev()
        .find_map(|event| match (&event.author, event.parts.first()) {
            (EventAuthor::Agent(_), Some(EventPart::Text(text))) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}
