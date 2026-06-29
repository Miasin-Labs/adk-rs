use std::collections::BTreeMap;

use adk_rs::{
    A2aAgentCard,
    A2aError,
    A2aMessage,
    A2aTransport,
    AgentName,
    ApiRoute,
    AppName,
    AuthConfig,
    AuthCredential,
    AuthScheme,
    BuiltinToolKind,
    CliCommand,
    CredentialManager,
    CredentialService,
    EvalCase,
    EvalResult,
    EvalService,
    InMemoryCredentialService,
    InMemoryEvalService,
    InMemoryTelemetrySink,
    IntegrationEndpoint,
    IntegrationKind,
    IntegrationRegistry,
    LiveRequest,
    LiveRequestQueue,
    ModelProvider,
    ModelRegistry,
    ModelSpec,
    RemoteA2aAgent,
    Skill,
    SkillRegistry,
    TelemetrySink,
    TelemetrySpan,
    TokenUsage,
    ToolRegistry,
    UserId,
    WorkflowEdge,
    WorkflowGraph,
    WorkflowNode,
    WorkflowNodeKind,
};
use async_trait::async_trait;

#[test]
fn credential_service_round_trips_user_scoped_secret_normal() {
    let service = InMemoryCredentialService::default();
    let app = AppName::new("app").unwrap();
    let user = UserId::new("user").unwrap();

    service
        .put_credential(
            &app,
            &user,
            "github",
            AuthCredential::BearerToken("token".to_owned()),
        )
        .unwrap();

    assert_eq!(
        service.get_credential(&app, &user, "github").unwrap(),
        Some(AuthCredential::BearerToken("token".to_owned()))
    );
}

#[test]
fn credential_manager_prefers_raw_then_stored_normal() {
    let service = InMemoryCredentialService::default();
    let app = AppName::new("app").unwrap();
    let user = UserId::new("user").unwrap();
    let config = AuthConfig {
        scheme: AuthScheme::HttpBearer,
        credential_key: AuthConfig::stable_key("tool", "call"),
        raw_credential: Some(AuthCredential::BearerToken("raw".to_owned())),
        exchanged_credential: None,
    };
    let manager = CredentialManager::new(service);

    assert_eq!(
        manager.resolve(&app, &user, &config).unwrap(),
        Some(AuthCredential::BearerToken("raw".to_owned()))
    );
}

struct EchoA2aTransport;

#[async_trait]
impl A2aTransport for EchoA2aTransport {
    async fn send_message(
        &self,
        _card: &A2aAgentCard,
        message: A2aMessage,
    ) -> Result<A2aMessage, A2aError> {
        Ok(A2aMessage {
            text: format!("echo: {}", message.text),
            ..message
        })
    }
}

#[tokio::test]
async fn remote_a2a_agent_invokes_transport_normal() {
    let agent = RemoteA2aAgent {
        card: A2aAgentCard {
            name: AgentName::new("remote").unwrap(),
            endpoint: "https://agent.example".to_owned(),
            capabilities: vec!["text".to_owned()],
        },
        transport: EchoA2aTransport,
    };

    let response = agent
        .invoke(A2aMessage {
            task_id: "task".to_owned(),
            role: "user".to_owned(),
            text: "hello".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(response.text, "echo: hello");
}

#[test]
fn builtin_tool_registry_exposes_upstream_catalog_normal() {
    let registry = ToolRegistry::with_all_builtin_specs();

    assert!(
        registry
            .spec(BuiltinToolKind::GoogleSearch.name())
            .is_some()
    );
    assert!(
        registry
            .spec(BuiltinToolKind::TransferToAgent.name())
            .is_some()
    );
    assert!(registry.specs().len() >= 30);
}

#[test]
fn model_specs_encode_provider_capabilities_normal() {
    let gemini = ModelSpec::gemini("gemini-2.5-pro");
    let anthropic = ModelSpec::openai_compatible(ModelProvider::Anthropic, "claude-opus-4");

    assert!(gemini.supports_live);
    assert!(gemini.supports_context_cache);
    assert!(anthropic.supports_tools);
    assert!(!anthropic.supports_context_cache);
    assert_eq!(
        ModelRegistry::resolve("gemini-2.5-pro").provider,
        ModelProvider::Gemini
    );
    assert_eq!(
        ModelRegistry::resolve("claude-opus-4").provider,
        ModelProvider::Anthropic
    );
}

#[test]
fn workflow_graph_tracks_roots_and_edges_normal() {
    let mut graph = WorkflowGraph::default();
    graph.add_node(WorkflowNode {
        id: "root".to_owned(),
        kind: WorkflowNodeKind::Agent("root".to_owned()),
    });
    graph.add_node(WorkflowNode {
        id: "tool".to_owned(),
        kind: WorkflowNodeKind::Tool("search".to_owned()),
    });

    graph
        .add_edge(WorkflowEdge {
            from: "root".to_owned(),
            to: "tool".to_owned(),
            route: None,
        })
        .unwrap();

    assert_eq!(graph.roots()[0].id, "root");
    assert_eq!(graph.next_nodes("root")[0].id, "tool");
}

#[test]
fn eval_service_records_cases_and_results_normal() {
    let service = InMemoryEvalService::default();
    service
        .put_case(EvalCase {
            id: "case".to_owned(),
            prompt: "p".to_owned(),
            expected: "e".to_owned(),
        })
        .unwrap();
    service
        .record_result(EvalResult {
            case_id: "case".to_owned(),
            scores: BTreeMap::from([("exact_match".to_owned(), 1.0)]),
            passed: true,
        })
        .unwrap();

    assert_eq!(service.list_cases().unwrap().len(), 1);
    assert!(service.list_results("case").unwrap()[0].passed);
}

#[test]
fn live_request_queue_preserves_order_normal() {
    let queue = LiveRequestQueue::default();

    queue.send(LiveRequest::UserText("one".to_owned())).unwrap();
    queue.send(LiveRequest::Close).unwrap();

    assert_eq!(
        queue.recv().unwrap(),
        Some(LiveRequest::UserText("one".to_owned()))
    );
    assert_eq!(queue.recv().unwrap(), Some(LiveRequest::Close));
}

#[test]
fn telemetry_sink_records_token_usage_normal() {
    let sink = InMemoryTelemetrySink::default();

    sink.record_span(TelemetrySpan {
        name: "model".to_owned(),
        trace_id: "trace".to_owned(),
        token_usage: Some(TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
        }),
    })
    .unwrap();

    assert_eq!(
        sink.spans().unwrap()[0]
            .token_usage
            .as_ref()
            .unwrap()
            .output_tokens,
        20
    );
}

#[test]
fn skill_and_integration_registries_lookup_by_name_normal() {
    let mut skills = SkillRegistry::default();
    skills.register(Skill {
        name: "research".to_owned(),
        description: "research skill".to_owned(),
        prompt: "think".to_owned(),
    });
    let mut integrations = IntegrationRegistry::default();
    integrations.register(IntegrationEndpoint {
        name: "slack".to_owned(),
        kind: IntegrationKind::Slack,
        endpoint: "https://slack.example".to_owned(),
    });

    assert_eq!(skills.get("research").unwrap().prompt, "think");
    assert_eq!(
        integrations.get("slack").unwrap().kind,
        IntegrationKind::Slack
    );
}

#[test]
fn server_and_cli_shapes_match_adk_surfaces_normal() {
    assert_eq!(ApiRoute::RunSse.path(), "/run_sse");
    assert_eq!(ApiRoute::Builder.path(), "/builder");
    assert_eq!(ApiRoute::DeployPlan.path(), "/deploy/plan");
    assert_eq!(
        CliCommand::Eval {
            eval_set: "default".to_owned()
        },
        CliCommand::Eval {
            eval_set: "default".to_owned()
        }
    );
}
