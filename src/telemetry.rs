use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySpan {
    pub name: String,
    pub trace_id: String,
    pub token_usage: Option<TokenUsage>,
}

pub trait TelemetrySink: Send + Sync {
    fn record_span(&self, span: TelemetrySpan) -> Result<(), TelemetryError>;
    fn spans(&self) -> Result<Vec<TelemetrySpan>, TelemetryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("telemetry sink lock poisoned")]
    Poisoned,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryTelemetrySink {
    spans: Arc<Mutex<Vec<TelemetrySpan>>>,
}

impl TelemetrySink for InMemoryTelemetrySink {
    fn record_span(&self, span: TelemetrySpan) -> Result<(), TelemetryError> {
        let mut guard = self.spans.lock().map_err(|_| TelemetryError::Poisoned)?;
        guard.push(span);
        Ok(())
    }

    fn spans(&self) -> Result<Vec<TelemetrySpan>, TelemetryError> {
        let guard = self.spans.lock().map_err(|_| TelemetryError::Poisoned)?;
        Ok(guard.clone())
    }
}
