use std::collections::BTreeMap;

use adk_rs::{
    AppName,
    ArtifactName,
    ArtifactService,
    Event,
    EventActions,
    EventAuthor,
    InMemoryArtifactService,
    InMemoryMemoryService,
    InMemorySessionStore,
    InvocationContext,
    InvocationId,
    MemoryEntry,
    MemoryService,
    RunConfig,
    Session,
    SessionId,
    SessionStore,
    StateKey,
    UserId,
};
use serde_json::json;

#[test]
fn session_append_applies_state_delta_normal() {
    let mut session = Session::for_user(
        AppName::new("app").unwrap(),
        UserId::new("user").unwrap(),
        SessionId::new("session").unwrap(),
    );
    let mut actions = EventActions::default();
    actions
        .state_delta
        .insert(StateKey::new("locale").unwrap(), json!("en-US"));

    session.append(
        Event::text(
            InvocationId::new("invocation").unwrap(),
            EventAuthor::User,
            "hello",
        )
        .with_actions(actions),
    );

    assert_eq!(session.events.len(), 1);
    assert_eq!(
        session.state[&StateKey::new("locale").unwrap()],
        json!("en-US")
    );
}

#[test]
fn artifact_versions_increment_normal() {
    let service = InMemoryArtifactService::default();
    let app = AppName::new("app").unwrap();
    let app_user = UserId::new("user").unwrap();
    let session = SessionId::new("session").unwrap();
    let name = ArtifactName::new("report.txt").unwrap();

    let first = service
        .save_artifact(
            &app,
            &app_user,
            Some(&session),
            name.clone(),
            b"one".to_vec(),
            "text/plain".to_owned(),
        )
        .unwrap();
    let second = service
        .save_artifact(
            &app,
            &app_user,
            Some(&session),
            name.clone(),
            b"two".to_vec(),
            "text/plain".to_owned(),
        )
        .unwrap();

    assert_eq!(first.0, 1);
    assert_eq!(second.0, 2);
    assert_eq!(
        service
            .load_artifact(&app, &app_user, Some(&session), &name, Some(second))
            .unwrap()
            .unwrap()
            .bytes,
        b"two".to_vec()
    );
    assert_eq!(
        service
            .list_artifact_keys(&app, &app_user, Some(&session))
            .unwrap(),
        vec![name.clone()]
    );
    service
        .delete_artifact(&app, &app_user, Some(&session), &name)
        .unwrap();
    assert!(
        service
            .list_versions(&app, &app_user, Some(&session), &name)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn memory_search_filters_by_user_and_query_normal() {
    let service = InMemoryMemoryService::default();
    let app = AppName::new("app").unwrap();
    let user = UserId::new("user").unwrap();
    service
        .add_memory(
            &app,
            &user,
            MemoryEntry {
                text: "Rust agents use typed events".to_owned(),
                metadata: BTreeMap::new(),
            },
        )
        .unwrap();

    let hits = service.search_memory(&app, &user, "typed").unwrap();

    assert_eq!(hits.len(), 1);
    assert!(hits[0].text.contains("typed events"));
}

#[test]
fn memory_can_ingest_session_events_normal() {
    let service = InMemoryMemoryService::default();
    let mut session = Session::for_user(
        AppName::new("app").unwrap(),
        UserId::new("user").unwrap(),
        SessionId::new("session").unwrap(),
    );
    session.append(Event::text(
        InvocationId::new("invocation").unwrap(),
        EventAuthor::User,
        "remember typed sessions",
    ));

    service.add_session_to_memory(&session).unwrap();
    let hits = service
        .search_memory(&session.app_name, &session.user_id, "sessions")
        .unwrap();

    assert_eq!(hits.len(), 1);
}

#[test]
fn invocation_context_enforces_llm_call_limit_robust() {
    let session = Session::for_user(
        AppName::new("app").unwrap(),
        UserId::new("user").unwrap(),
        SessionId::new("session").unwrap(),
    );
    let config = RunConfig {
        max_llm_calls: Some(1),
        ..RunConfig::default()
    };
    let mut context = InvocationContext::new(
        &session,
        InvocationId::new("invocation").unwrap(),
        adk_rs::AgentName::new("agent").unwrap(),
    )
    .with_run_config(config);

    context.increment_llm_call_count().unwrap();
    assert!(context.increment_llm_call_count().is_err());
}

#[test]
fn in_memory_session_store_appends_and_loads_normal() {
    let store = InMemorySessionStore::default();
    let session = Session::for_user(
        AppName::new("app").unwrap(),
        UserId::new("user").unwrap(),
        SessionId::new("session").unwrap(),
    );
    store.create(session.clone()).unwrap();

    let saved = store
        .append_event(
            &session.id,
            Event::text(
                InvocationId::new("invocation").unwrap(),
                EventAuthor::User,
                "hello",
            ),
        )
        .unwrap();

    assert_eq!(saved.events.len(), 1);
    assert_eq!(store.load(&session.id).unwrap().unwrap().events.len(), 1);
}
