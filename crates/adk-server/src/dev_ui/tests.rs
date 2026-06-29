use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde_json::{Value, json};

pub async fn list() -> Json<Vec<String>> {
    Json(Vec::new())
}

pub async fn get(Path((_, test)): Path<(String, String)>) -> Json<Vec<Value>> {
    Json(vec![json!({ "testName": test, "events": [] })])
}

pub async fn put(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({ "ok": true, "test": body }))
}

pub async fn delete() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn rebuild() -> Json<Value> {
    Json(json!({ "ok": true, "rebuilt": [] }))
}

pub async fn run() -> String {
    "No Rust compatibility tests are registered.\n".to_owned()
}
