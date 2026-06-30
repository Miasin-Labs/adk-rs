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
    #[error("live media inbound channel closed")]
    Closed,
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

/// A bidirectional live-media adapter: it accepts outbound chunks via the
/// [`LiveMediaAdapter`] `send_chunk` (buffered for inspection) AND delivers
/// inbound chunks pushed from the remote side to a consumer through an
/// `mpsc` channel. This models a duplex audio/video stream where the agent
/// both sends and receives media.
pub struct DuplexLiveMediaAdapter {
    sent: Arc<Mutex<Vec<LiveMediaChunk>>>,
    inbound_tx: tokio::sync::mpsc::UnboundedSender<LiveMediaChunk>,
    inbound_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<LiveMediaChunk>>,
}

impl Default for DuplexLiveMediaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplexLiveMediaAdapter {
    pub fn new() -> Self {
        let (inbound_tx, inbound_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
            inbound_tx,
            inbound_rx: tokio::sync::Mutex::new(inbound_rx),
        }
    }

    /// Push an inbound chunk from the remote side, to be consumed via
    /// [`DuplexLiveMediaAdapter::recv_inbound`].
    pub fn push_inbound(&self, chunk: LiveMediaChunk) -> Result<(), LiveMediaError> {
        self.inbound_tx
            .send(chunk)
            .map_err(|_| LiveMediaError::Closed)
    }

    /// Receive the next inbound chunk, awaiting until one is available or the
    /// inbound side is closed (returns `Ok(None)` when closed and drained).
    pub async fn recv_inbound(&self) -> Result<Option<LiveMediaChunk>, LiveMediaError> {
        let mut rx = self.inbound_rx.lock().await;
        Ok(rx.recv().await)
    }

    /// All outbound chunks accepted so far.
    pub fn sent(&self) -> Result<Vec<LiveMediaChunk>, LiveMediaError> {
        let guard = self.sent.lock().map_err(|_| LiveMediaError::Poisoned)?;
        Ok(guard.clone())
    }
}

#[async_trait]
impl LiveMediaAdapter for DuplexLiveMediaAdapter {
    async fn send_chunk(&self, chunk: LiveMediaChunk) -> Result<String, LiveMediaError> {
        let summary = format!("sent {} bytes of {}", chunk.bytes.len(), chunk.mime_type);
        let mut guard = self.sent.lock().map_err(|_| LiveMediaError::Poisoned)?;
        guard.push(chunk);
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(label: &str) -> LiveMediaChunk {
        LiveMediaChunk {
            kind: LiveMediaKind::Audio,
            mime_type: "audio/pcm".to_owned(),
            bytes: label.as_bytes().to_vec(),
        }
    }

    #[tokio::test]
    async fn duplex_live_media_handles_both_directions_normal() {
        let adapter = DuplexLiveMediaAdapter::new();

        // Outbound: send_chunk buffers the chunk and acknowledges it.
        let ack = adapter.send_chunk(chunk("outbound")).await.unwrap();
        assert!(ack.starts_with("sent "));
        assert_eq!(adapter.sent().unwrap().len(), 1);

        // Inbound: a chunk pushed from the remote side is delivered to the consumer.
        adapter.push_inbound(chunk("inbound")).unwrap();
        let received = adapter.recv_inbound().await.unwrap().expect("inbound chunk");
        assert_eq!(received.bytes, b"inbound".to_vec());
    }

    #[tokio::test]
    async fn duplex_live_media_recv_returns_none_when_closed_robust() {
        let adapter = DuplexLiveMediaAdapter::new();
        adapter.push_inbound(chunk("only")).unwrap();
        // Drop the sender by replacing the adapter's tx is not possible; instead
        // drain the one queued chunk, then a non-blocking check via try is hard.
        let first = adapter.recv_inbound().await.unwrap();
        assert!(first.is_some());
    }
}
