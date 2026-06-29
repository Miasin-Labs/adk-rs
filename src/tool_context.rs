use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::auth::{AuthCredential, AuthError, CredentialService};
use crate::event::EventActions;
use crate::ids::{AppName, InvocationId, SessionId, UserId};

#[derive(Debug, Clone)]
pub struct ReadonlyContext {
    pub app_name: AppName,
    pub user_id: UserId,
    pub session_id: SessionId,
    pub invocation_id: InvocationId,
    pub state: BTreeMap<String, Value>,
}

#[derive(Clone)]
pub struct ToolContext {
    pub readonly: ReadonlyContext,
    pub actions: EventActions,
    pub credential_service: Option<Arc<dyn CredentialService>>,
}

impl ToolContext {
    pub fn state(&self, key: &str) -> Option<&Value> {
        self.readonly.state.get(key)
    }

    pub fn actions_mut(&mut self) -> &mut EventActions {
        &mut self.actions
    }

    pub fn credential(&self, key: &str) -> Result<Option<AuthCredential>, AuthError> {
        let Some(service) = &self.credential_service else {
            return Ok(None);
        };
        service.get_credential(&self.readonly.app_name, &self.readonly.user_id, key)
    }
}
