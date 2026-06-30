use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::event::{Event, EventAuthor, EventPart};
use crate::model::{LanguageModel, ModelError, ModelRequest, ModelResponse};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub events: Vec<Event>,
}

pub trait RecordingStore: Send + Sync {
    fn put(&self, id: &str, events: Vec<Event>) -> Result<Recording, RecordingError>;
    fn get(&self, id: &str) -> Result<Option<Recording>, RecordingError>;
    fn list(&self) -> Result<Vec<Recording>, RecordingError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error("recording store lock poisoned")]
    Poisoned,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryRecordingStore {
    recordings: Arc<Mutex<BTreeMap<String, Recording>>>,
}

impl RecordingStore for InMemoryRecordingStore {
    fn put(&self, id: &str, events: Vec<Event>) -> Result<Recording, RecordingError> {
        let recording = Recording {
            id: id.to_owned(),
            events,
        };
        let mut guard = self
            .recordings
            .lock()
            .map_err(|_| RecordingError::Poisoned)?;
        guard.insert(id.to_owned(), recording.clone());
        Ok(recording)
    }

    fn get(&self, id: &str) -> Result<Option<Recording>, RecordingError> {
        let guard = self
            .recordings
            .lock()
            .map_err(|_| RecordingError::Poisoned)?;
        Ok(guard.get(id).cloned())
    }

    fn list(&self) -> Result<Vec<Recording>, RecordingError> {
        let guard = self
            .recordings
            .lock()
            .map_err(|_| RecordingError::Poisoned)?;
        Ok(guard.values().cloned().collect())
    }
}

pub struct ReplayCursor {
    recording: Recording,
    index: usize,
}

impl ReplayCursor {
    pub fn new(recording: Recording) -> Self {
        Self {
            recording,
            index: 0,
        }
    }

    pub fn next_event(&mut self) -> Option<Event> {
        let event = self.recording.events.get(self.index).cloned();
        if event.is_some() {
            self.index += 1;
        }
        event
    }
}

/// A [`LanguageModel`] that replays the agent responses from a recording in
/// order, instead of calling a live provider. Each `generate` call returns the
/// next agent-authored response reconstructed from the recorded events
/// (text + tool calls), so a recorded run can be re-executed deterministically.
///
/// When the recording is exhausted, further calls return an empty response
/// (no text, no tool calls), which terminates the run loop cleanly.
pub struct ReplayModel {
    responses: Mutex<std::collections::VecDeque<ModelResponse>>,
}

impl ReplayModel {
    /// Build a replay model from a recording, extracting one `ModelResponse`
    /// per agent-authored event (its text and any tool calls).
    pub fn new(recording: Recording) -> Self {
        let responses = recording
            .events
            .iter()
            .filter(|event| matches!(event.author, EventAuthor::Agent(_)))
            .map(response_from_event)
            .collect();
        Self {
            responses: Mutex::new(responses),
        }
    }

    /// Number of agent responses still queued for replay.
    pub fn remaining(&self) -> usize {
        self.responses.lock().map(|q| q.len()).unwrap_or(0)
    }
}

fn response_from_event(event: &Event) -> ModelResponse {
    let mut text = None;
    let mut tool_calls = Vec::new();
    for part in &event.parts {
        match part {
            EventPart::Text(value) => text = Some(value.clone()),
            EventPart::ToolCall(call) => tool_calls.push(call.clone()),
            EventPart::ToolResult(_) => {}
        }
    }
    ModelResponse {
        text,
        tool_calls,
        actions: event.actions.clone(),
    }
}

#[async_trait]
impl LanguageModel for ReplayModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut queue = self
            .responses
            .lock()
            .map_err(|_| ModelError::Failed("replay model lock poisoned".to_owned()))?;
        // Exhausted recording -> empty response stops the run loop.
        Ok(queue.pop_front().unwrap_or(ModelResponse {
            text: None,
            tool_calls: Vec::new(),
            actions: crate::event::EventActions::default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventActions;
    use crate::ids::{AgentName, InvocationId};

    fn agent_text_event(text: &str) -> Event {
        Event {
            id: crate::ids::EventId::for_index(0),
            invocation_id: InvocationId::new("inv").unwrap(),
            author: EventAuthor::Agent(AgentName::new("assistant").unwrap()),
            parts: vec![EventPart::Text(text.to_owned())],
            actions: EventActions::default(),
            timestamp_seconds: 0,
        }
    }

    #[tokio::test]
    async fn replay_model_reproduces_recorded_agent_text_in_order_normal() {
        let recording = Recording {
            id: "rec1".to_owned(),
            events: vec![
                Event::text(
                    InvocationId::new("inv").unwrap(),
                    EventAuthor::User,
                    "hi",
                ),
                agent_text_event("first"),
                agent_text_event("second"),
            ],
        };
        let model = ReplayModel::new(recording);
        assert_eq!(model.remaining(), 2); // only the 2 agent events

        let req = || ModelRequest {
            instruction: String::new(),
            events: Vec::new(),
            tools: Vec::new(),
        };
        assert_eq!(model.generate(req()).await.unwrap().text.as_deref(), Some("first"));
        assert_eq!(model.generate(req()).await.unwrap().text.as_deref(), Some("second"));
        // Exhausted -> empty response.
        assert_eq!(model.generate(req()).await.unwrap().text, None);
    }
}
