//! File-backed n8n credential store + REST CRUD, and the credential-type
//! catalog served at `/types/credentials.json`.
//!
//! Secrets are stored in plaintext under the dev data dir — this is a
//! loopback-only dev server. The list/create responses omit `data`; only
//! `GET /rest/credentials/:id` returns it (mirroring the editor's expectation).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::DevUiState;

pub(crate) struct CredentialStore {
    dir: PathBuf,
    credentials: Mutex<BTreeMap<String, Value>>,
    next_id: AtomicU64,
}

impl CredentialStore {
    pub(crate) fn load(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let mut credentials = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "json")
                    && let Ok(text) = std::fs::read_to_string(entry.path())
                    && let Ok(credential) = serde_json::from_str::<Value>(&text)
                    && let Some(id) = credential.get("id").and_then(Value::as_str)
                {
                    credentials.insert(id.to_owned(), credential.clone());
                }
            }
        }
        let next_id = super::next_id_after(credentials.keys(), "cred-");
        Self {
            dir,
            credentials: Mutex::new(credentials),
            next_id: AtomicU64::new(next_id),
        }
    }

    fn persist(&self, id: &str, credential: &Value) {
        if let Ok(text) = serde_json::to_string_pretty(credential) {
            let _ = std::fs::write(self.dir.join(format!("{id}.json")), text);
        }
    }

    fn new_id(&self) -> String {
        format!("cred-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// The decrypted `data` object for a credential, for execution-time use.
    pub(crate) fn data(&self, id: &str) -> Option<Value> {
        self.credentials
            .lock()
            .unwrap()
            .get(id)
            .and_then(|credential| credential.get("data").cloned())
    }
}

/// Strip the secret `data` field for list/create responses.
fn sanitize(credential: &Value) -> Value {
    let mut clone = credential.clone();
    if let Some(object) = clone.as_object_mut() {
        object.remove("data");
    }
    clone
}

pub(crate) async fn list(State(state): State<DevUiState>) -> Json<Value> {
    let items: Vec<Value> = state
        .credentials
        .credentials
        .lock()
        .unwrap()
        .values()
        .map(sanitize)
        .collect();
    Json(json!({ "data": items }))
}

pub(crate) async fn for_workflow(State(state): State<DevUiState>) -> Json<Value> {
    list(State(state)).await
}

#[derive(Deserialize)]
pub(crate) struct NewNameQuery {
    name: Option<String>,
}

pub(crate) async fn new_name(Query(query): Query<NewNameQuery>) -> Json<Value> {
    let name = query.name.unwrap_or_else(|| "My credential".to_owned());
    Json(json!({ "data": { "name": name } }))
}

pub(crate) async fn create(
    State(state): State<DevUiState>,
    Json(mut body): Json<Value>,
) -> Json<Value> {
    let id = state.credentials.new_id();
    let now = super::iso_now();
    if let Some(object) = body.as_object_mut() {
        object.insert("id".into(), json!(id));
        object.insert("createdAt".into(), json!(now));
        object.insert("updatedAt".into(), json!(now));
        object.entry("name").or_insert(json!("My credential"));
        object.entry("type").or_insert(json!("httpHeaderAuth"));
        object.entry("data").or_insert(json!({}));
    }
    state.credentials.persist(&id, &body);
    state
        .credentials
        .credentials
        .lock()
        .unwrap()
        .insert(id, body.clone());
    Json(json!({ "data": sanitize(&body) }))
}

pub(crate) async fn get_one(
    Path(id): Path<String>,
    State(state): State<DevUiState>,
) -> Result<Json<Value>, StatusCode> {
    state
        .credentials
        .credentials
        .lock()
        .unwrap()
        .get(&id)
        .map(|credential| Json(json!({ "data": credential })))
        .ok_or(StatusCode::NOT_FOUND)
}

pub(crate) async fn update(
    Path(id): Path<String>,
    State(state): State<DevUiState>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mut credentials = state.credentials.credentials.lock().unwrap();
    let credential = credentials.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    if let (Some(target), Some(source)) = (credential.as_object_mut(), patch.as_object()) {
        for (key, value) in source {
            if key != "id" {
                target.insert(key.clone(), value.clone());
            }
        }
        target.insert("updatedAt".into(), json!(super::iso_now()));
    }
    let out = credential.clone();
    state.credentials.persist(&id, &out);
    Ok(Json(json!({ "data": sanitize(&out) })))
}

pub(crate) async fn delete(Path(id): Path<String>, State(state): State<DevUiState>) -> Json<Value> {
    state.credentials.credentials.lock().unwrap().remove(&id);
    let _ = std::fs::remove_file(state.credentials.dir.join(format!("{id}.json")));
    Json(json!({ "data": true }))
}

/// `GET /types/credentials.json` — bare array of credential-type descriptions.
pub(crate) async fn credential_types() -> Json<Value> {
    Json(json!([
        {
            "name": "httpHeaderAuth",
            "displayName": "Header Auth",
            "documentationUrl": "",
            "properties": [
                { "displayName": "Name", "name": "name", "type": "string", "default": "" },
                { "displayName": "Value", "name": "value", "type": "string", "typeOptions": { "password": true }, "default": "" }
            ]
        },
        {
            "name": "openAiApi",
            "displayName": "OpenAI API",
            "documentationUrl": "",
            "properties": [
                { "displayName": "API Key", "name": "apiKey", "type": "string", "typeOptions": { "password": true }, "default": "" }
            ]
        }
    ]))
}
