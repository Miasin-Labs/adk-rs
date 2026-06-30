//! Integration test: a run records its events to a RecordingStore, and a
//! ReplayModel driven by that recording reproduces the agent outputs without a
//! live model.

use std::sync::Arc;

use adk_rs::{
    AgentBuilder,
    AgentName,
    EventActions,
    EventAuthor,
    EventPart,
    InMemoryRecordingStore,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    RecordingStore,
    ReplayModel,
    Runner,
    SessionId,
};
use async_trait::async_trait;

/// A live model that answers with a fixed line; used to produce the original
/// run that we then record and replay.
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

fn agent_text(output: &adk_rs::RunOutput) -> String {
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

#[tokio::test]
async fn run_records_then_replay_model_reproduces_output_normal() {
    let store = Arc::new(InMemoryRecordingStore::default());

    // 1. Run with a live model, recording the run under "rec-1".
    let live = AgentBuilder::new(
        AgentName::new("assistant").unwrap(),
        "answer the question",
        Arc::new(FixedModel { answer: "the answer is 42" }),
    )
    .build()
    .unwrap();
    let recorded_output = Runner::new(InMemorySessionStore::default(), live)
        .record_to(store.clone() as Arc<dyn RecordingStore>, "rec-1")
        .run(
            &SessionId::new("s1").unwrap(),
            InvocationId::new("i1").unwrap(),
            "what is the answer?",
        )
        .await
        .unwrap();
    let original = agent_text(&recorded_output);
    assert_eq!(original, "the answer is 42");

    // 2. The store has the recording.
    let recording = store.get("rec-1").unwrap().expect("recording persisted");
    assert!(!recording.events.is_empty());

    // 3. Replay the recording through a ReplayModel — no live model involved.
    let replay_agent = AgentBuilder::new(
        AgentName::new("assistant").unwrap(),
        "answer the question",
        Arc::new(ReplayModel::new(recording)),
    )
    .build()
    .unwrap();
    let replayed_output = Runner::new(InMemorySessionStore::default(), replay_agent)
        .run(
            &SessionId::new("s2").unwrap(),
            InvocationId::new("i2").unwrap(),
            "what is the answer?",
        )
        .await
        .unwrap();

    // The replayed agent output matches the originally recorded output.
    assert_eq!(agent_text(&replayed_output), original);
}
