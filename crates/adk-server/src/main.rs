// The n8n FrontendSettings payload is a single large nested `json!` literal.
#![recursion_limit = "512"]

use std::net::SocketAddr;

use adk_rs::{ExactMatchEvaluator, MetricEvaluator, MetricInput, VisualAgentBuilder};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tower_http::cors::CorsLayer;

mod dev_ui;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::args()
        .skip_while(|arg| arg != "--port")
        .nth(1)
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(8091);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app()).await?;
    Ok(())
}

fn app() -> Router {
    let dev_ui_state = dev_ui::DevUiState::default();
    Router::new()
        .route("/health", get(health))
        .route("/routes", get(routes))
        .route("/builder/sample-dot", get(builder_dot))
        .route("/builder/parse", post(builder_parse))
        .route("/recordings", get(recordings))
        .route("/metrics", get(metrics))
        .route("/metrics/evaluate", post(metric_evaluate))
        .route("/deploy/plan", post(deploy_plan))
        .merge(dev_ui::router(dev_ui_state))
        .layer(CorsLayer::permissive())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn routes() -> Json<Vec<&'static str>> {
    Json(vec![
        adk_rs::ApiRoute::Run.path(),
        adk_rs::ApiRoute::RunSse.path(),
        adk_rs::ApiRoute::Live.path(),
        adk_rs::ApiRoute::Artifacts.path(),
        adk_rs::ApiRoute::Sessions.path(),
        adk_rs::ApiRoute::Builder.path(),
        adk_rs::ApiRoute::Recordings.path(),
        adk_rs::ApiRoute::Metrics.path(),
        adk_rs::ApiRoute::DeployPlan.path(),
    ])
}

async fn builder_dot() -> Json<serde_json::Value> {
    Json(json!({ "dot": "digraph adk_agent { root -> critic; }" }))
}

async fn builder_parse(Json(request): Json<BuilderRequest>) -> Json<serde_json::Value> {
    match VisualAgentBuilder::parse_yaml(&request.yaml)
        .and_then(|blueprint| VisualAgentBuilder::to_dot(&blueprint).map(|dot| (blueprint, dot)))
    {
        Ok((blueprint, dot)) => Json(json!({ "ok": true, "name": blueprint.name, "dot": dot })),
        Err(error) => Json(json!({ "ok": false, "error": error.to_string() })),
    }
}

async fn recordings() -> Json<serde_json::Value> {
    Json(json!({ "recordings": [] }))
}

async fn metrics() -> Json<serde_json::Value> {
    Json(json!({ "metrics": ["exact_match", "safety", "hallucination", "trajectory"] }))
}

async fn metric_evaluate(Json(request): Json<MetricRequest>) -> Json<serde_json::Value> {
    let input = MetricInput {
        expected: request.expected,
        actual: request.actual,
        expected_tools: Vec::new(),
        actual_tools: Vec::new(),
        forbidden_terms: Vec::new(),
        grounded_terms: Vec::new(),
    };
    Json(json!(ExactMatchEvaluator.evaluate(&input)))
}

async fn deploy_plan(Json(request): Json<DeployRequest>) -> Json<serde_json::Value> {
    Json(json!({
        "service": request.service,
        "target": request.target,
        "steps": ["Resolve credentials", "Package service", "Deploy service"]
    }))
}

#[derive(Debug, Deserialize)]
struct BuilderRequest {
    yaml: String,
}

#[derive(Debug, Deserialize)]
struct MetricRequest {
    expected: String,
    actual: String,
}

#[derive(Debug, Deserialize)]
struct DeployRequest {
    service: String,
    target: String,
}
