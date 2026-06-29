use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::guardrail::Guardrail;
use crate::structured_output::StructuredOutputSchema;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamingMode {
    #[default]
    None,
    Sse,
    Bidirectional,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub streaming_mode: StreamingMode,
    pub max_llm_calls: Option<u32>,
    pub max_iterations: Option<u32>,
    pub memory_window_events: Option<usize>,
    pub structured_output_schema: Option<StructuredOutputSchema>,
    #[serde(skip)]
    pub guardrails: Vec<Arc<dyn Guardrail>>,
    pub save_input_blobs_as_artifacts: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            streaming_mode: StreamingMode::None,
            max_llm_calls: Some(100),
            max_iterations: Some(100),
            memory_window_events: None,
            structured_output_schema: None,
            guardrails: Vec::new(),
            save_input_blobs_as_artifacts: false,
        }
    }
}

impl fmt::Debug for RunConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunConfig")
            .field("streaming_mode", &self.streaming_mode)
            .field("max_llm_calls", &self.max_llm_calls)
            .field("max_iterations", &self.max_iterations)
            .field("memory_window_events", &self.memory_window_events)
            .field("structured_output_schema", &self.structured_output_schema)
            .field("guardrails", &self.guardrails.len())
            .field(
                "save_input_blobs_as_artifacts",
                &self.save_input_blobs_as_artifacts,
            )
            .finish()
    }
}
