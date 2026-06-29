use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::event::Event;

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
