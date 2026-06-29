use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamingMode {
    #[default]
    None,
    Sse,
    Bidirectional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConfig {
    pub streaming_mode: StreamingMode,
    pub max_llm_calls: Option<u32>,
    pub save_input_blobs_as_artifacts: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            streaming_mode: StreamingMode::None,
            max_llm_calls: Some(100),
            save_input_blobs_as_artifacts: false,
        }
    }
}
