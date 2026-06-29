use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::event::Event;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveRequest {
    UserText(String),
    AudioChunk(Vec<u8>),
    VideoChunk(Vec<u8>),
    ToolResponse(String),
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveResponse {
    Event(Event),
    Transcript(String),
    Closed,
}

#[derive(Debug, Default, Clone)]
pub struct LiveRequestQueue {
    queue: Arc<Mutex<VecDeque<LiveRequest>>>,
}

impl LiveRequestQueue {
    pub fn send(&self, request: LiveRequest) -> Result<(), LiveError> {
        let mut guard = self.queue.lock().map_err(|_| LiveError::Poisoned)?;
        guard.push_back(request);
        Ok(())
    }

    pub fn recv(&self) -> Result<Option<LiveRequest>, LiveError> {
        let mut guard = self.queue.lock().map_err(|_| LiveError::Poisoned)?;
        Ok(guard.pop_front())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LiveError {
    #[error("live queue lock poisoned")]
    Poisoned,
}
