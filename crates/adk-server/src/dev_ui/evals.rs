use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde_json::{Value, json};

use super::types::EvalSetCreateRequest;

pub async fn metrics_info() -> Json<Value> {
    Json(json!({ "metricsInfo": [
        { "metricName": "exact_match", "description": "Exact string match", "metricValueInfo": { "interval": { "minValue": 0, "maxValue": 1 } } },
        { "metricName": "tool_trajectory", "description": "Tool-call sequence agreement", "metricValueInfo": { "interval": { "minValue": 0, "maxValue": 1 } } }
    ] }))
}

pub async fn sets() -> Json<Vec<Value>> {
    Json(Vec::new())
}

pub async fn create_set(Json(request): Json<EvalSetCreateRequest>) -> Json<Value> {
    Json(json!({ "ok": true, "evalSet": request.eval_set.unwrap_or_else(|| json!({})) }))
}

pub async fn set(Path((_, eval_set)): Path<(String, String)>) -> Json<Value> {
    Json(json!({ "evalSetId": eval_set, "evalCases": [] }))
}

pub async fn delete_set() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn cases() -> Json<Vec<Value>> {
    Json(Vec::new())
}

pub async fn case(Path((_, eval_set, eval_case)): Path<(String, String, String)>) -> Json<Value> {
    Json(json!({ "evalSetId": eval_set, "evalId": eval_case, "conversation": [] }))
}

pub async fn update_case(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({ "ok": true, "evalCase": body }))
}

pub async fn delete_case() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn add_session(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({ "ok": true, "request": body }))
}

pub async fn run_eval(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({ "ok": true, "evalResults": [], "request": body }))
}

pub async fn results() -> Json<Vec<Value>> {
    Json(Vec::new())
}

pub async fn result(Path((_, result_id)): Path<(String, String)>) -> Json<Value> {
    Json(json!({ "evalResultId": result_id, "results": [] }))
}
