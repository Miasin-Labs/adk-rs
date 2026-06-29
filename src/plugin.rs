use async_trait::async_trait;

use crate::event::Event;
use crate::invocation::InvocationContext;
use crate::model::{ModelRequest, ModelResponse};
use crate::tool::{ToolCall, ToolResult};

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;

    async fn on_user_message(
        &self,
        _context: &InvocationContext,
        message: String,
    ) -> Result<String, PluginError> {
        Ok(message)
    }

    async fn before_run(&self, _context: &InvocationContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn on_event(
        &self,
        _context: &InvocationContext,
        event: Event,
    ) -> Result<Event, PluginError> {
        Ok(event)
    }

    async fn after_run(&self, _context: &InvocationContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn before_model(
        &self,
        _context: &InvocationContext,
        _request: &ModelRequest,
    ) -> Result<Option<ModelResponse>, PluginError> {
        Ok(None)
    }

    async fn after_model(
        &self,
        _context: &InvocationContext,
        response: ModelResponse,
    ) -> Result<ModelResponse, PluginError> {
        Ok(response)
    }

    async fn before_tool(
        &self,
        _context: &InvocationContext,
        _call: &ToolCall,
    ) -> Result<Option<ToolResult>, PluginError> {
        Ok(None)
    }

    async fn after_tool(
        &self,
        _context: &InvocationContext,
        result: ToolResult,
    ) -> Result<ToolResult, PluginError> {
        Ok(result)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin {plugin} failed: {message}")]
    Failed { plugin: String, message: String },
}
