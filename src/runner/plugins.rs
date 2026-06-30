use std::sync::Arc;

use crate::agent::Agent;
use crate::event::Event;
use crate::invocation::InvocationContext;
use crate::model::{ModelRequest, ModelResponse};
use crate::runner::{RunError, Runner};
use crate::session::{Session, SessionStore};
use crate::tool::{ToolCall, ToolResult};

impl<S: SessionStore> Runner<S> {
    pub(super) async fn before_run(&self, context: &InvocationContext) -> Result<(), RunError> {
        for plugin in &self.plugins {
            plugin.before_run(context).await?;
        }
        Ok(())
    }

    pub(super) async fn on_user_message(
        &self,
        context: &InvocationContext,
        mut message: String,
    ) -> Result<String, RunError> {
        for plugin in &self.plugins {
            message = plugin.on_user_message(context, message).await?;
        }
        Ok(message)
    }

    pub(super) async fn after_run(&self, context: &InvocationContext) -> Result<(), RunError> {
        for plugin in &self.plugins {
            plugin.after_run(context).await?;
        }
        Ok(())
    }

    pub(super) async fn generate_model_response(
        &self,
        context: &InvocationContext,
        agent: &Agent,
        request: ModelRequest,
    ) -> Result<ModelResponse, RunError> {
        for plugin in &self.plugins {
            if let Some(response) = plugin.before_model(context, &request).await? {
                return Ok(response);
            }
        }
        let mut response = agent.model.generate(request).await?;
        for plugin in &self.plugins {
            response = plugin.after_model(context, response).await?;
        }
        Ok(response)
    }

    pub(super) async fn call_tool(
        &self,
        context: &InvocationContext,
        session: &Session,
        agent: &Agent,
        call: &ToolCall,
    ) -> Result<ToolResult, RunError> {
        for plugin in &self.plugins {
            if let Some(result) = plugin.before_tool(context, call).await? {
                return Ok(result);
            }
        }
        let tool = agent
            .tools
            .iter()
            .find(|tool| tool.spec().name == call.name)
            .ok_or_else(|| RunError::UnknownTool(call.name.clone()))?;
        let mut tool_context = super::context::tool_context(
            context,
            session,
            self.credential_service.as_ref().map(Arc::clone),
        );
        let mut result = tool.call_with_context(call, &mut tool_context).await?;
        for plugin in &self.plugins {
            result = plugin.after_tool(context, result).await?;
        }
        Ok(result)
    }

    pub(super) async fn emit_event(
        &self,
        context: &InvocationContext,
        session: &mut Session,
        mut event: Event,
    ) -> Result<Event, RunError> {
        for plugin in &self.plugins {
            event = plugin.on_event(context, event).await?;
        }
        session.append(event.clone());
        // Forward to the streaming sink, if `Runner::stream` is driving this run.
        // A closed receiver (dropped stream consumer) is not an error.
        if let Some(sink) = &context.event_sink {
            let _ = sink.send(event.clone());
        }
        Ok(event)
    }
}
