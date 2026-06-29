//! `pushRef`-keyed SSE registry and the `/rest/push` handler.
//!
//! The editor opens `EventSource('/rest/push?pushRef=<uuid>')` on boot and
//! sends that same id back as the `push-ref` header when it runs a workflow.
//! We map `pushRef -> channel` so the run handler can stream execution
//! messages to exactly the tab that started the run.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::super::DevUiState;

#[derive(Default)]
pub(crate) struct PushRegistry {
    clients: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
}

impl PushRegistry {
    fn register(&self, push_ref: String) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.clients.lock().unwrap().insert(push_ref, tx);
        rx
    }

    fn unregister(&self, push_ref: &str) {
        self.clients.lock().unwrap().remove(push_ref);
    }

    /// Fire-and-forget one n8n `PushMessage` (`{ type, data }`) to a pushRef.
    pub(crate) fn send(&self, push_ref: &str, message: &Value) {
        if let Some(tx) = self.clients.lock().unwrap().get(push_ref) {
            let _ = tx.send(message.to_string());
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PushQuery {
    #[serde(rename = "pushRef")]
    push_ref: Option<String>,
}

/// Removes a tab's sender from the registry when its SSE stream is dropped.
struct UnregisterGuard {
    registry: Arc<PushRegistry>,
    push_ref: String,
}

impl Drop for UnregisterGuard {
    fn drop(&mut self) {
        self.registry.unregister(&self.push_ref);
    }
}

pub(crate) async fn push(
    State(state): State<DevUiState>,
    Query(query): Query<PushQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let push_ref = query.push_ref.unwrap_or_else(|| "default".to_owned());
    let receiver = state.push.register(push_ref.clone());
    let guard = UnregisterGuard {
        registry: state.push.clone(),
        push_ref,
    };
    // Each yielded String is already a serialized `{type,data}`; axum frames it
    // as `data: <json>\n\n`. The guard rides along in the stream state so a
    // closed tab unregisters itself.
    let stream = futures::stream::unfold((receiver, guard), |(mut receiver, guard)| async move {
        receiver
            .recv()
            .await
            .map(|json| (Ok::<_, Infallible>(Event::default().data(json)), (receiver, guard)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
