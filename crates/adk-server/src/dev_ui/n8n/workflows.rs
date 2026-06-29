//! File-backed n8n workflow store + REST CRUD handlers.
//!
//! Workflows are mirrored to one JSON file per workflow under a data dir and
//! reloaded at startup. Response envelopes match the editor's expectations:
//! the list is `{ data: { count, data: [] } }`; single ops are `{ data: {...} }`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use super::super::DevUiState;

pub(crate) struct WorkflowStore {
    dir: PathBuf,
    workflows: Mutex<BTreeMap<String, Value>>,
    executions: Mutex<BTreeMap<String, Value>>,
    /// Suspended executions (Wait node), keyed by execution id.
    waiting: Mutex<BTreeMap<String, Value>>,
    next_id: AtomicU64,
    next_exec: AtomicU64,
}

impl WorkflowStore {
    pub(crate) fn load(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let mut workflows = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "json")
                    && let Ok(text) = std::fs::read_to_string(entry.path())
                    && let Ok(workflow) = serde_json::from_str::<Value>(&text)
                    && let Some(id) = workflow.get("id").and_then(Value::as_str)
                {
                    workflows.insert(id.to_owned(), workflow.clone());
                }
            }
        }
        let next_id = super::next_id_after(workflows.keys(), "wf-");
        Self {
            dir,
            workflows: Mutex::new(workflows),
            executions: Mutex::new(BTreeMap::new()),
            waiting: Mutex::new(BTreeMap::new()),
            next_id: AtomicU64::new(next_id),
            next_exec: AtomicU64::new(1),
        }
    }

    fn persist(&self, id: &str, workflow: &Value) {
        if let Ok(text) = serde_json::to_string_pretty(workflow) {
            let _ = std::fs::write(self.dir.join(format!("{id}.json")), text);
        }
    }

    pub(crate) fn all(&self) -> Vec<Value> {
        self.workflows.lock().unwrap().values().cloned().collect()
    }

    pub(crate) fn get(&self, id: &str) -> Option<Value> {
        self.workflows.lock().unwrap().get(id).cloned()
    }

    fn new_id(&self) -> String {
        format!("wf-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    pub(crate) fn next_execution_id(&self) -> String {
        self.next_exec.fetch_add(1, Ordering::Relaxed).to_string()
    }

    pub(crate) fn record_execution(&self, id: String, data: Value) {
        self.executions.lock().unwrap().insert(id, data);
    }

    pub(crate) fn execution(&self, id: &str) -> Option<Value> {
        self.executions.lock().unwrap().get(id).cloned()
    }

    /// Save a suspended (Wait node) execution's state.
    pub(crate) fn save_waiting(&self, id: &str, state: Value) {
        self.waiting.lock().unwrap().insert(id.to_owned(), state);
    }

    /// Whether an execution is currently suspended.
    pub(crate) fn peek_waiting(&self, id: &str) -> Option<()> {
        self.waiting.lock().unwrap().contains_key(id).then_some(())
    }

    /// Remove and return a suspended execution's state (for resume).
    pub(crate) fn take_waiting(&self, id: &str) -> Option<Value> {
        self.waiting.lock().unwrap().remove(id)
    }
}

/// Fields the editor reads back on every workflow object, filled in if absent.
fn enrich(workflow: &mut Value) {
    let Some(object) = workflow.as_object_mut() else {
        return;
    };
    object.entry("active").or_insert(json!(false));
    object.entry("isArchived").or_insert(json!(false));
    object.entry("nodes").or_insert(json!([]));
    object.entry("connections").or_insert(json!({}));
    object
        .entry("settings")
        .or_insert(json!({ "executionOrder": "v1" }));
    object.entry("tags").or_insert(json!([]));
    object.entry("pinData").or_insert(json!({}));
    object.entry("meta").or_insert(json!({}));
    object.insert("versionId".into(), json!(super::version_id()));
    object.insert(
        "scopes".into(),
        json!([
            "workflow:read",
            "workflow:update",
            "workflow:delete",
            "workflow:execute"
        ]),
    );
    object.insert(
        "homeProject".into(),
        json!({
            "id": "proj-personal",
            "name": "Personal",
            "type": "personal",
            "icon": null
        }),
    );
    object.insert("sharedWithProjects".into(), json!([]));
}

pub(crate) async fn list(State(state): State<DevUiState>) -> Json<Value> {
    let items = state.workflows.all();
    Json(json!({ "data": { "count": items.len(), "data": items } }))
}

pub(crate) async fn create(
    State(state): State<DevUiState>,
    Json(mut body): Json<Value>,
) -> Json<Value> {
    let id = state.workflows.new_id();
    let now = super::iso_now();
    if let Some(object) = body.as_object_mut() {
        object.insert("id".into(), json!(id));
        object.insert("createdAt".into(), json!(now));
        object.insert("updatedAt".into(), json!(now));
        object.entry("name").or_insert(json!("My workflow"));
    }
    enrich(&mut body);
    state.workflows.persist(&id, &body);
    state.workflows.workflows.lock().unwrap().insert(id, body.clone());
    Json(json!({ "data": body }))
}

pub(crate) async fn get_one(
    Path(id): Path<String>,
    State(state): State<DevUiState>,
) -> Result<Json<Value>, StatusCode> {
    state
        .workflows
        .get(&id)
        .map(|workflow| Json(json!({ "data": workflow })))
        .ok_or(StatusCode::NOT_FOUND)
}

pub(crate) async fn update(
    Path(id): Path<String>,
    State(state): State<DevUiState>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mut workflows = state.workflows.workflows.lock().unwrap();
    let workflow = workflows.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    if let (Some(target), Some(source)) = (workflow.as_object_mut(), patch.as_object()) {
        for (key, value) in source {
            if key != "id" {
                target.insert(key.clone(), value.clone());
            }
        }
        target.insert("updatedAt".into(), json!(super::iso_now()));
        target.insert("versionId".into(), json!(super::version_id()));
    }
    let out = workflow.clone();
    state.workflows.persist(&id, &out);
    Ok(Json(json!({ "data": out })))
}

pub(crate) async fn delete(Path(id): Path<String>, State(state): State<DevUiState>) -> Json<Value> {
    state.workflows.workflows.lock().unwrap().remove(&id);
    let _ = std::fs::remove_file(state.workflows.dir.join(format!("{id}.json")));
    Json(json!({ "data": true }))
}

/// `GET /rest/executions/:id` — the canvas may fetch the full run output.
pub(crate) async fn get_execution(
    Path(id): Path<String>,
    State(state): State<DevUiState>,
) -> Json<Value> {
    Json(json!({ "data": state.workflows.execution(&id).unwrap_or(json!({})) }))
}
