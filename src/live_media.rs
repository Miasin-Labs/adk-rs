use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveMediaKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveMediaChunk {
    pub kind: LiveMediaKind,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[async_trait]
pub trait LiveMediaAdapter: Send + Sync {
    async fn send_chunk(&self, chunk: LiveMediaChunk) -> Result<String, LiveMediaError>;
}

#[derive(Debug, thiserror::Error)]
pub enum LiveMediaError {
    #[error("live media adapter lock poisoned")]
    Poisoned,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryLiveMediaAdapter {
    chunks: Arc<Mutex<Vec<LiveMediaChunk>>>,
}

impl InMemoryLiveMediaAdapter {
    pub fn chunks(&self) -> Result<Vec<LiveMediaChunk>, LiveMediaError> {
        let guard = self.chunks.lock().map_err(|_| LiveMediaError::Poisoned)?;
        Ok(guard.clone())
    }
}

#[async_trait]
impl LiveMediaAdapter for InMemoryLiveMediaAdapter {
    async fn send_chunk(&self, chunk: LiveMediaChunk) -> Result<String, LiveMediaError> {
        let summary = format!(
            "accepted {} bytes of {}",
            chunk.bytes.len(),
            chunk.mime_type
        );
        let mut guard = self.chunks.lock().map_err(|_| LiveMediaError::Poisoned)?;
        guard.push(chunk);
        Ok(summary)
    }
}
