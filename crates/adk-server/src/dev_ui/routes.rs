use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::types::{
    CreateSessionRequest,
    ResumeApprovalRequest,
    RunAgentRequest,
    UpdateSessionRequest,
    parse_body,
};
use super::{DevUiState, agent, artifacts, evals, graph, n8n, tests, traces};

const DEFAULT_APP: &str = "hello_world";

pub fn router(state: DevUiState) -> Router {
    Router::new()
        .route("/dev-ui/config", get(ui_config))
        .route("/version", get(version))
        .route("/list-apps", get(|| async { Json(vec![DEFAULT_APP]) }))
        .route("/apps/{app_name}/app-info", get(app_info))
        .route("/apps/{app_name}/users/{user_id}/sessions", get(list_sessions).post(create_session))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}", get(get_session).patch(update_session).delete(delete_session))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}/artifacts", get(artifacts::list))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}/artifacts/{artifact_name}", get(artifacts::latest).delete(artifacts::delete))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}/artifacts/{artifact_name}/versions", get(artifacts::versions))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}/artifacts/{artifact_name}/versions/metadata", get(artifacts::versions_metadata))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}/artifacts/{artifact_name}/versions/{version_id}", get(artifacts::version))
        .route("/apps/{app_name}/users/{user_id}/sessions/{session_id}/artifacts/{artifact_name}/versions/{version_id}/metadata", get(artifacts::version_metadata))
        .route("/run_sse", post(run_sse))
        .route("/run", post(run))
        .route("/run_live", get(live_unavailable))
        .route("/dev/apps/{app_name}/build_graph", get(build_graph))
        .route("/dev/apps/{app_name}/build_graph_image", get(build_graph_image))
        .route("/dev/apps/{app_name}/builder", get(builder_yaml))
        .route("/dev/apps/{app_name}/builder/save", post(builder_save))
        .route("/dev/apps/{app_name}/builder/cancel", post(builder_cancel))
        .route("/dev/apps/{app_name}/approvals/{approval_id}/resume", post(resume_approval))
        .route("/dev/apps/{app_name}/metrics-info", get(evals::metrics_info))
        .route("/dev/apps/{app_name}/eval_sets", get(evals::sets))
        .route("/dev/apps/{app_name}/eval-sets", post(evals::create_set))
        .route("/dev/apps/{app_name}/eval_sets/{eval_set_id}", get(evals::set).delete(evals::delete_set))
        .route("/dev/apps/{app_name}/eval_sets/{eval_set_id}/evals", get(evals::cases))
        .route("/dev/apps/{app_name}/eval_sets/{eval_set_id}/evals/{eval_case_id}", get(evals::case).put(evals::update_case).delete(evals::delete_case))
        .route("/dev/apps/{app_name}/eval_sets/{eval_set_id}/add_session", post(evals::add_session))
        .route("/dev/apps/{app_name}/eval_sets/{eval_set_id}/run_eval", post(evals::run_eval))
        .route("/dev/apps/{app_name}/eval_results", get(evals::results))
        .route("/dev/apps/{app_name}/eval_results/{eval_result_id}", get(evals::result))
        .route("/dev/apps/{app_name}/tests", get(tests::list))
        .route("/dev/apps/{app_name}/tests/rebuild", post(tests::rebuild))
        .route("/dev/apps/{app_name}/tests/run", post(tests::run))
        .route("/dev/apps/{app_name}/tests/{test_name}", get(tests::get).put(tests::put).delete(tests::delete))
        .route("/dev/apps/{app_name}/debug/trace/{event_id}", get(trace_event))
        .route("/dev/apps/{app_name}/debug/trace/session/{session_id}", get(trace_session))
        .route("/dev/apps/{app_name}/users/{user_id}/sessions/{session_id}/events/{event_id}/graph", get(event_graph))
        // The verbatim n8n editor-ui SPA: REST/types/push surface, static asset
        // mounts, and an SPA-aware fallback for client-side routes (served at
        // the site root).
        .merge(n8n::router())
        .merge(n8n::static_routes())
        .fallback(n8n::spa_fallback)
        .with_state(state)
}

async fn ui_config() -> Json<Value> {
    Json(json!({ "logo_text": "Agent Development Kit", "logo_image_url": null }))
}

async fn version() -> Json<Value> {
    Json(json!({ "version": "2.2.0", "language": "rust", "language_version": "2024" }))
}

async fn app_info(Path(app_name): Path<String>) -> Json<Value> {
    Json(agent::app_info(&app_name))
}

async fn list_sessions(
    Path((app_name, user_id)): Path<(String, String)>,
    State(state): State<DevUiState>,
) -> Json<Vec<super::types::DevSession>> {
    Json(state.list_sessions(&app_name, &user_id).await)
}

async fn create_session(
    Path((app_name, user_id)): Path<(String, String)>,
    State(state): State<DevUiState>,
    body: Bytes,
) -> Json<super::types::DevSession> {
    Json(
        state
            .create_session(app_name, user_id, parse_body::<CreateSessionRequest>(&body))
            .await,
    )
}

async fn get_session(
    Path((_, _, session_id)): Path<(String, String, String)>,
    State(state): State<DevUiState>,
) -> Result<Json<super::types::DevSession>, StatusCode> {
    state
        .get_session(&session_id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn update_session(
    Path((_, _, session_id)): Path<(String, String, String)>,
    State(state): State<DevUiState>,
    Json(request): Json<UpdateSessionRequest>,
) -> Result<Json<super::types::DevSession>, StatusCode> {
    state
        .update_session(&session_id, request.state_delta)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_session(
    Path((_, _, session_id)): Path<(String, String, String)>,
    State(state): State<DevUiState>,
) -> StatusCode {
    state.delete_session(&session_id).await;
    StatusCode::NO_CONTENT
}

async fn run(
    State(state): State<DevUiState>,
    Json(request): Json<RunAgentRequest>,
) -> Json<Vec<Value>> {
    Json(state.run_events(&request).await)
}

async fn run_sse(
    State(state): State<DevUiState>,
    Json(request): Json<RunAgentRequest>,
) -> Response {
    let mut body = String::new();
    for event in state.run_events(&request).await {
        match serde_json::to_string(&event) {
            Ok(serialized) => body.push_str(&format!("data: {serialized}\n\n")),
            Err(error) => body.push_str(&format!("data: {{\"error\":\"{error}\"}}\n\n")),
        }
    }
    let mut headers = cors_headers();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    (headers, body).into_response()
}

async fn live_unavailable() -> Json<Value> {
    Json(
        json!({ "error": "run_live websocket is not implemented in the Rust compatibility server yet" }),
    )
}

async fn build_graph(Path(app_name): Path<String>) -> Json<Value> {
    Json(agent::build_graph(&app_name))
}

async fn build_graph_image() -> Json<Value> {
    Json(json!({ "": { "dotSrc": graph::HELLO_WORLD_DOT } }))
}

async fn builder_yaml(Path(app_name): Path<String>) -> String {
    agent::builder_yaml(&app_name)
}

async fn builder_save() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn builder_cancel() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn resume_approval(
    Path((_, approval_id)): Path<(String, String)>,
    State(state): State<DevUiState>,
    Json(request): Json<ResumeApprovalRequest>,
) -> Json<Vec<Value>> {
    Json(
        state
            .resume_approval(&request.session_id, &approval_id, request.approved)
            .await,
    )
}

async fn trace_event(
    Path((_, event_id)): Path<(String, String)>,
    State(state): State<DevUiState>,
) -> Json<Value> {
    Json(traces::event_trace(&state, &event_id).await)
}

async fn trace_session(
    Path((_, session_id)): Path<(String, String)>,
    State(state): State<DevUiState>,
) -> Json<Vec<Value>> {
    Json(traces::session_trace(&state, &session_id).await)
}

async fn event_graph(
    Path((_, _, _, event_id)): Path<(String, String, String, String)>,
    State(state): State<DevUiState>,
) -> Json<Value> {
    Json(graph::event_graph(
        state.event_by_id(&event_id).await.as_ref(),
    ))
}

fn cors_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,PUT,PATCH,DELETE,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type,accept"),
    );
    headers
}
