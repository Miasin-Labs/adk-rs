use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde_json::{Value, json};

pub async fn list() -> Json<Vec<Value>> {
    Json(Vec::new())
}

pub async fn latest(Path((_, _, _, name)): Path<(String, String, String, String)>) -> Json<Value> {
    Json(artifact(&name, "0"))
}

pub async fn version(
    Path((_, _, _, name, version)): Path<(String, String, String, String, String)>,
) -> Json<Value> {
    Json(artifact(&name, &version))
}

pub async fn versions() -> Json<Vec<String>> {
    Json(vec!["0".to_owned()])
}

pub async fn versions_metadata() -> Json<Vec<Value>> {
    Json(vec![json!({ "version": 0, "mimeType": "text/plain" })])
}

pub async fn version_metadata(
    Path((_, _, _, name, version)): Path<(String, String, String, String, String)>,
) -> Json<Value> {
    Json(json!({ "name": name, "version": version, "mimeType": "text/plain" }))
}

pub async fn delete() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn artifact(name: &str, version: &str) -> Value {
    json!({
        "id": name,
        "versionId": version,
        "mimeType": "text/plain",
        "text": format!("Artifact {name} version {version} is not persisted by the Rust compatibility server yet."),
    })
}
