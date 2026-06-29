use std::sync::Arc;

use async_trait::async_trait;

use crate::auth::AuthScheme;
use crate::model::ModelRequest;
use crate::tool::{Tool, ToolError};
use crate::tool_context::{ReadonlyContext, ToolContext};

#[async_trait]
pub trait Toolset: Send + Sync {
    async fn tools(
        &self,
        context: Option<&ReadonlyContext>,
    ) -> Result<Vec<Arc<dyn Tool>>, ToolError>;

    async fn process_model_request(
        &self,
        _context: &mut ToolContext,
        _request: &mut ModelRequest,
    ) -> Result<(), ToolError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), ToolError> {
        Ok(())
    }

    fn auth_scheme(&self) -> Option<AuthScheme> {
        None
    }
}
