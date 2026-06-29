use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::event::{Event, EventActions};
use crate::tool::{ToolCall, ToolSpec};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelProvider {
    Gemini,
    VertexAi,
    Anthropic,
    OpenAi,
    LiteLlm,
    Apigee,
    Gemma,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub provider: ModelProvider,
    pub model: String,
    pub supports_live: bool,
    pub supports_tools: bool,
    pub supports_context_cache: bool,
}

impl ModelSpec {
    pub fn gemini(model: impl Into<String>) -> Self {
        Self {
            provider: ModelProvider::Gemini,
            model: model.into(),
            supports_live: true,
            supports_tools: true,
            supports_context_cache: true,
        }
    }

    pub fn openai_compatible(provider: ModelProvider, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            supports_live: false,
            supports_tools: true,
            supports_context_cache: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ModelRegistry;

impl ModelRegistry {
    pub fn resolve(model: &str) -> ModelSpec {
        if let Some((provider, model)) = model.split_once('/') {
            return ModelSpec::openai_compatible(Self::provider_from_prefix(provider), model);
        }
        if model.starts_with("gemini-")
            || model.starts_with("gemma-4")
            || model.starts_with("model-optimizer-")
        {
            return ModelSpec::gemini(model);
        }
        if model.starts_with("gemma-") {
            return ModelSpec::openai_compatible(ModelProvider::Gemma, model);
        }
        if model.starts_with("claude-") {
            return ModelSpec::openai_compatible(ModelProvider::Anthropic, model);
        }
        if model.starts_with("gpt-") || model.starts_with("o1-") || model.starts_with("o3-") {
            return ModelSpec::openai_compatible(ModelProvider::OpenAi, model);
        }
        ModelSpec::openai_compatible(ModelProvider::LiteLlm, model)
    }

    fn provider_from_prefix(provider: &str) -> ModelProvider {
        match provider {
            "apigee" => ModelProvider::Apigee,
            "anthropic" | "claude" => ModelProvider::Anthropic,
            "gemini" | "google" => ModelProvider::Gemini,
            "gemma" => ModelProvider::Gemma,
            "openai" => ModelProvider::OpenAi,
            "vertex_ai" | "vertex" => ModelProvider::VertexAi,
            "lite" | "litellm" => ModelProvider::LiteLlm,
            other => ModelProvider::Custom(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub instruction: String,
    pub events: Vec<Event>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub actions: EventActions,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model failed: {0}")]
    Failed(String),
    #[error("unsupported model capability {0}")]
    UnsupportedCapability(String),
}

#[async_trait]
pub trait LanguageModel: Send + Sync {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;
}
