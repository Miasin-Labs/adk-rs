use std::sync::Arc;

use async_trait::async_trait;

use crate::model::{LanguageModel, ModelError, ModelRequest, ModelResponse};

#[derive(Clone)]
pub struct FallbackLanguageModel {
    models: Vec<Arc<dyn LanguageModel>>,
}

impl FallbackLanguageModel {
    pub fn new(models: Vec<Arc<dyn LanguageModel>>) -> Self {
        Self { models }
    }
}

#[async_trait]
impl LanguageModel for FallbackLanguageModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut errors = Vec::new();
        for model in &self.models {
            match model.generate(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) => errors.push(error.to_string()),
            }
        }
        Err(ModelError::Failed(format!(
            "all fallback models failed: {}",
            errors.join("; ")
        )))
    }
}
