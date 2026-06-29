use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::approvals::{DevPendingApproval, approval_requested};
use super::events;
use super::openai::{AgentRun, OpenAiAgent};
use super::types::{CreateSessionRequest, DevSession, RunAgentRequest};

#[derive(Clone)]
pub struct DevUiState {
    pub(super) sessions: Arc<Mutex<BTreeMap<String, DevSession>>>,
    next_session: Arc<AtomicU64>,
    next_event: Arc<AtomicU64>,
    model: Arc<Option<OpenAiAgent>>,
    pub(super) pending_approvals: Arc<Mutex<BTreeMap<String, DevPendingApproval>>>,
    /// File-backed n8n workflow store (serves `/rest/workflows`).
    pub(super) workflows: Arc<super::n8n::workflows::WorkflowStore>,
    /// File-backed n8n credential store (serves `/rest/credentials`).
    pub(super) credentials: Arc<super::n8n::credentials::CredentialStore>,
    /// pushRef-keyed SSE registry (drives the n8n canvas run animation).
    pub(super) push: Arc<super::n8n::push::PushRegistry>,
    /// In-memory namespace store backing the n8n "ADK Memory" node.
    pub(super) memory: Arc<std::sync::Mutex<BTreeMap<String, Vec<Value>>>>,
}

impl DevUiState {
    pub async fn list_sessions(&self, app_name: &str, user_id: &str) -> Vec<DevSession> {
        self.sessions
            .lock()
            .await
            .values()
            .filter(|session| session.app_name == app_name && session.user_id == user_id)
            .cloned()
            .collect()
    }

    pub async fn create_session(
        &self,
        app_name: String,
        user_id: String,
        request: Option<CreateSessionRequest>,
    ) -> DevSession {
        let id = request
            .as_ref()
            .and_then(|request| request.session_id.clone())
            .unwrap_or_else(|| self.new_session_id());
        let session = DevSession {
            id: id.clone(),
            app_name,
            user_id,
            state: request
                .as_ref()
                .and_then(|request| request.state.clone())
                .unwrap_or_else(|| json!({})),
            events: request
                .and_then(|request| request.events)
                .unwrap_or_default(),
            last_update_time: now_seconds(),
        };
        self.sessions.lock().await.insert(id, session.clone());
        session
    }

    pub async fn get_session(&self, session_id: &str) -> Option<DevSession> {
        self.sessions.lock().await.get(session_id).cloned()
    }

    pub async fn update_session(
        &self,
        session_id: &str,
        delta: Option<Value>,
    ) -> Option<DevSession> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(session_id)?;
        if let Some(delta) = delta {
            merge_object(&mut session.state, delta);
        }
        session.last_update_time = now_seconds();
        Some(session.clone())
    }

    pub async fn delete_session(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    pub async fn run_events(&self, request: &RunAgentRequest) -> Vec<Value> {
        let invocation_id = self.new_event_id();
        let prompt = events::request_message_text(request);
        if let Some(delta) = request.state_delta.clone() {
            let _ = self.update_session(&request.session_id, Some(delta)).await;
        }
        let mut response_events = self.events_for(&invocation_id, &prompt, request).await;
        let mut persisted_events = Vec::with_capacity(response_events.len() + 1);
        persisted_events.push(events::user_event(self, &invocation_id, &prompt));
        persisted_events.extend(response_events.iter().cloned());
        self.persist_events(&request.session_id, &persisted_events)
            .await;
        response_events.shrink_to_fit();
        response_events
    }

    async fn events_for(
        &self,
        invocation_id: &str,
        user_text: &str,
        request: &RunAgentRequest,
    ) -> Vec<Value> {
        if approval_requested(user_text) {
            return vec![
                self.create_approval_event(invocation_id, &request.session_id, user_text)
                    .await,
            ];
        }
        let mut rolls = self.session_rolls(&request.session_id).await;
        match self.model.as_ref() {
            Some(model) => match model.run(user_text, &mut rolls).await {
                Ok(run) => self.model_events(invocation_id, run),
                Err(error) => vec![events::error_event(self, invocation_id, &error)],
            },
            None => vec![events::error_event(
                self,
                invocation_id,
                "OpenAI is not configured. Set OPENAI_API_KEY and optionally ADK_OPENAI_MODEL.",
            )],
        }
    }

    fn model_events(&self, invocation_id: &str, run: AgentRun) -> Vec<Value> {
        let mut events = Vec::new();
        for observation in run.tools {
            events.push(events::tool_call_event(self, invocation_id, &observation));
            events.push(events::tool_response_event(
                self,
                invocation_id,
                &observation,
            ));
        }
        events.push(events::agent_event(self, invocation_id, &run.text));
        events
    }

    pub(super) async fn persist_events(&self, session_id: &str, events: &[Value]) {
        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
            for event in events {
                apply_state_delta(&mut session.state, event);
            }
            session.events.extend(events.iter().cloned());
            session.last_update_time = now_seconds();
        }
    }

    async fn session_rolls(&self, session_id: &str) -> Vec<i64> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .and_then(|session| session.state.get("rolls"))
            .and_then(Value::as_array)
            .map(|rolls| rolls.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default()
    }

    pub async fn session_events(&self, session_id: &str) -> Vec<Value> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.events.clone())
            .unwrap_or_default()
    }

    pub async fn event_by_id(&self, event_id: &str) -> Option<Value> {
        self.sessions
            .lock()
            .await
            .values()
            .flat_map(|session| session.events.iter())
            .find(|event| event.get("id").and_then(Value::as_str) == Some(event_id))
            .cloned()
    }

    /// The configured dev_ui model, if any, for the n8n "ADK Agent" node.
    pub(super) fn agent(&self) -> Option<&OpenAiAgent> {
        self.model.as_ref().as_ref()
    }

    /// All items stored in a memory namespace (ADK Memory node, retrieve).
    pub(super) fn memory_all(&self, namespace: &str) -> Vec<Value> {
        self.memory
            .lock()
            .unwrap()
            .get(namespace)
            .cloned()
            .unwrap_or_default()
    }

    /// Append items to a memory namespace (ADK Memory node, store).
    pub(super) fn memory_append(&self, namespace: &str, items: &[Value]) {
        self.memory
            .lock()
            .unwrap()
            .entry(namespace.to_owned())
            .or_default()
            .extend(items.iter().cloned());
    }

    fn new_session_id(&self) -> String {
        let sequence = self.next_session.fetch_add(1, Ordering::Relaxed);
        format!("rust-session-{sequence}")
    }

    pub(crate) fn new_event_id(&self) -> String {
        let sequence = self.next_event.fetch_add(1, Ordering::Relaxed);
        format!("rust-event-{sequence}")
    }
}

impl Default for DevUiState {
    fn default() -> Self {
        let base_dir = std::env::var("ADK_N8N_DATA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.n8n-data"))
            });
        Self {
            sessions: Arc::default(),
            next_session: Arc::default(),
            next_event: Arc::default(),
            model: Arc::new(OpenAiAgent::load()),
            pending_approvals: Arc::default(),
            workflows: Arc::new(super::n8n::workflows::WorkflowStore::load(
                base_dir.join("workflows"),
            )),
            credentials: Arc::new(super::n8n::credentials::CredentialStore::load(
                base_dir.join("credentials"),
            )),
            push: Arc::default(),
            memory: Arc::default(),
        }
    }
}

fn merge_object(target: &mut Value, delta: Value) {
    let (Some(target), Some(delta)) = (target.as_object_mut(), delta.as_object()) else {
        return;
    };
    target.extend(
        delta
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
}

fn apply_state_delta(state: &mut Value, event: &Value) {
    let Some(delta) = event
        .get("actions")
        .and_then(|actions| actions.get("stateDelta"))
        .cloned()
    else {
        return;
    };
    merge_object(state, delta);
}

pub(crate) fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}
