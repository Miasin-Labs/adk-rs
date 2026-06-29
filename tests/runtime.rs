use std::collections::BTreeMap;

use adk_rs::{
    ArtifactName,
    ArtifactService,
    EvalCase,
    EvalResult,
    EvalService,
    FileArtifactService,
    FileEvalService,
    FileSessionStore,
    InvocationId,
    Session,
    SessionId,
    SessionStore,
    StreamingResponseAggregator,
    WorkflowEdge,
    WorkflowGraph,
    WorkflowNode,
    WorkflowNodeKind,
    WorkflowRuntime,
};
use tempfile::tempdir;

#[test]
fn streaming_aggregator_combines_partial_text_until_final_normal() {
    let mut aggregator = StreamingResponseAggregator::default();

    aggregator.push_partial_text("hel");
    aggregator.push_partial_text("lo");
    let response = aggregator.finish(InvocationId::new("invocation").unwrap());

    assert_eq!(response.text, Some("hello".to_owned()));
    assert!(response.tool_calls.is_empty());
}

#[test]
fn workflow_runtime_executes_reachable_nodes_in_order_normal() {
    let mut graph = WorkflowGraph::default();
    graph.add_node(WorkflowNode {
        id: "root".to_owned(),
        kind: WorkflowNodeKind::Function("root_fn".to_owned()),
    });
    graph.add_node(WorkflowNode {
        id: "child".to_owned(),
        kind: WorkflowNodeKind::Tool("search".to_owned()),
    });
    graph
        .add_edge(WorkflowEdge {
            from: "root".to_owned(),
            to: "child".to_owned(),
            route: None,
        })
        .unwrap();

    let runtime = WorkflowRuntime::new(graph);
    let visited = runtime.run_from_roots().unwrap();

    assert_eq!(visited, vec!["root".to_owned(), "child".to_owned()]);
}

#[test]
fn file_session_store_persists_session_events_normal() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(dir.path());
    let session = Session::new(SessionId::new("session").unwrap());

    store.create(session.clone()).unwrap();
    store
        .append_event(
            &session.id,
            adk_rs::Event::text(
                InvocationId::new("invocation").unwrap(),
                adk_rs::EventAuthor::User,
                "hello",
            ),
        )
        .unwrap();

    assert_eq!(store.load(&session.id).unwrap().unwrap().events.len(), 1);
}

#[test]
fn file_eval_service_persists_cases_and_results_normal() {
    let dir = tempdir().unwrap();
    let service = FileEvalService::new(dir.path());

    service
        .put_case(EvalCase {
            id: "case".to_owned(),
            prompt: "question".to_owned(),
            expected: "answer".to_owned(),
        })
        .unwrap();
    service
        .record_result(EvalResult {
            case_id: "case".to_owned(),
            scores: BTreeMap::from([("score".to_owned(), 1.0)]),
            passed: true,
        })
        .unwrap();

    let reopened = FileEvalService::new(dir.path());
    assert_eq!(reopened.list_cases().unwrap().len(), 1);
    assert!(reopened.list_results("case").unwrap()[0].passed);
}

#[test]
fn file_artifact_service_persists_latest_version_normal() {
    let dir = tempdir().unwrap();
    let service = FileArtifactService::new(dir.path());
    let app = adk_rs::AppName::new("app").unwrap();
    let user = adk_rs::UserId::new("user").unwrap();
    let name = ArtifactName::new("report.txt").unwrap();

    service
        .save_artifact(
            &app,
            &user,
            None,
            name.clone(),
            b"one".to_vec(),
            "text/plain".to_owned(),
        )
        .unwrap();
    service
        .save_artifact(
            &app,
            &user,
            None,
            name.clone(),
            b"two".to_vec(),
            "text/plain".to_owned(),
        )
        .unwrap();

    let reopened = FileArtifactService::new(dir.path());
    assert_eq!(
        reopened
            .load_artifact(&app, &user, None, &name, None)
            .unwrap()
            .unwrap()
            .bytes,
        b"two".to_vec()
    );
}
