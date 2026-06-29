use adk_rs::{
    CloudCredential,
    CloudTarget,
    ConfiguredCloudBackend,
    DeploymentBackend,
    Event,
    EventAuthor,
    ExactMatchEvaluator,
    HallucinationEvaluator,
    InMemoryLiveMediaAdapter,
    InMemoryRecordingStore,
    InvocationId,
    LiveMediaAdapter,
    LiveMediaChunk,
    LiveMediaKind,
    MetricEvaluator,
    MetricInput,
    RecordingStore,
    ReplayCursor,
    SafetyEvaluator,
    TrajectoryEvaluator,
    VisualAgentBuilder,
};

#[test]
fn visual_agent_builder_parses_yaml_and_emits_dot_normal() {
    let yaml = r#"
name: root
model: gemini-2.5-pro
instruction: answer clearly
tools: [google_search]
sub_agents:
  - name: critic
    model: claude-opus-4
    instruction: critique answer
"#;

    let blueprint = VisualAgentBuilder::parse_yaml(yaml).unwrap();
    let dot = VisualAgentBuilder::to_dot(&blueprint).unwrap();

    assert_eq!(blueprint.name, "root");
    assert!(dot.contains("root -> critic"));
}

#[test]
fn replay_cursor_replays_recorded_events_in_order_normal() {
    let store = InMemoryRecordingStore::default();
    let event = Event::text(
        InvocationId::new("invocation").unwrap(),
        EventAuthor::User,
        "hello",
    );
    store.put("recording", vec![event.clone()]).unwrap();

    let mut cursor = ReplayCursor::new(store.get("recording").unwrap().unwrap());

    assert_eq!(cursor.next_event(), Some(event));
    assert_eq!(cursor.next_event(), None);
}

#[tokio::test]
async fn live_media_adapter_accepts_audio_and_video_chunks_normal() {
    let adapter = InMemoryLiveMediaAdapter::default();

    let audio = adapter
        .send_chunk(LiveMediaChunk {
            kind: LiveMediaKind::Audio,
            mime_type: "audio/pcm".to_owned(),
            bytes: vec![1, 2, 3],
        })
        .await
        .unwrap();
    let video = adapter
        .send_chunk(LiveMediaChunk {
            kind: LiveMediaKind::Video,
            mime_type: "video/raw".to_owned(),
            bytes: vec![4, 5],
        })
        .await
        .unwrap();

    assert!(audio.contains("audio/pcm"));
    assert!(video.contains("video/raw"));
    assert_eq!(adapter.chunks().unwrap().len(), 2);
}

#[test]
fn cloud_backend_builds_deployment_plan_when_credentials_present_normal() {
    let backend = ConfiguredCloudBackend::new(CloudCredential {
        project_id: "project".to_owned(),
        region: "us-central1".to_owned(),
        bearer_token: Some("token".to_owned()),
    });

    let plan = backend
        .plan_deploy(CloudTarget::CloudRun, "adk-app")
        .unwrap();

    assert_eq!(plan.service_name, "adk-app");
    assert!(plan.steps.iter().any(|step| step.contains("Cloud Run")));
}

#[test]
fn metric_evaluators_score_exact_safety_hallucination_and_trajectory_normal() {
    let input = MetricInput {
        expected: "final answer".to_owned(),
        actual: "final answer".to_owned(),
        expected_tools: vec!["google_search".to_owned()],
        actual_tools: vec!["google_search".to_owned()],
        forbidden_terms: vec!["unsafe".to_owned()],
        grounded_terms: vec!["final".to_owned(), "answer".to_owned()],
    };

    assert!(ExactMatchEvaluator.evaluate(&input).passed);
    assert!(SafetyEvaluator.evaluate(&input).passed);
    assert!(HallucinationEvaluator.evaluate(&input).passed);
    assert!(TrajectoryEvaluator.evaluate(&input).passed);
}
