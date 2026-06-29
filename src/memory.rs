use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::event::{Event, EventPart};
use crate::ids::{AppName, UserId};
use crate::session::Session;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub text: String,
    pub metadata: BTreeMap<String, String>,
}

pub trait MemoryService: Send + Sync {
    fn add_session_to_memory(&self, session: &Session) -> Result<(), MemoryError>;

    fn add_events_to_memory(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        events: &[Event],
    ) -> Result<(), MemoryError>;

    fn add_memory(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        entry: MemoryEntry,
    ) -> Result<(), MemoryError>;

    fn search_memory(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        query: &str,
    ) -> Result<Vec<MemoryEntry>, MemoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("memory store lock poisoned")]
    Poisoned,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryMemoryService {
    entries: Arc<Mutex<BTreeMap<String, Vec<MemoryEntry>>>>,
}

impl MemoryService for InMemoryMemoryService {
    fn add_session_to_memory(&self, session: &Session) -> Result<(), MemoryError> {
        self.add_events_to_memory(&session.app_name, &session.user_id, &session.events)
    }

    fn add_events_to_memory(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        events: &[Event],
    ) -> Result<(), MemoryError> {
        let entries = events
            .iter()
            .filter_map(event_text)
            .map(|text| MemoryEntry {
                text,
                metadata: BTreeMap::new(),
            });
        let mut guard = self.entries.lock().map_err(|_| MemoryError::Poisoned)?;
        guard
            .entry(memory_key(app_name, user_id))
            .or_default()
            .extend(entries);
        Ok(())
    }

    fn add_memory(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        entry: MemoryEntry,
    ) -> Result<(), MemoryError> {
        let mut guard = self.entries.lock().map_err(|_| MemoryError::Poisoned)?;
        guard
            .entry(memory_key(app_name, user_id))
            .or_default()
            .push(entry);
        Ok(())
    }

    fn search_memory(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        query: &str,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let guard = self.entries.lock().map_err(|_| MemoryError::Poisoned)?;
        let needle = query.to_ascii_lowercase();
        Ok(guard
            .get(&memory_key(app_name, user_id))
            .into_iter()
            .flatten()
            .filter(|entry| entry.text.to_ascii_lowercase().contains(&needle))
            .cloned()
            .collect())
    }
}

fn memory_key(app_name: &AppName, user_id: &UserId) -> String {
    format!("{}:{}", app_name.as_str(), user_id.as_str())
}

fn event_text(event: &Event) -> Option<String> {
    event.parts.iter().find_map(|part| match part {
        EventPart::Text(text) => Some(text.clone()),
        EventPart::ToolCall(_) | EventPart::ToolResult(_) => None,
    })
}
