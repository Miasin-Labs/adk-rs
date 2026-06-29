use std::collections::BTreeMap;

use serde_json::Value;

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

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub readonly: ReadonlyContext,
    pub actions: EventActions,
}

impl ToolContext {
    pub fn state(&self, key: &str) -> Option<&Value> {
        self.readonly.state.get(key)
    }

    pub fn actions_mut(&mut self) -> &mut EventActions {
        &mut self.actions
    }
}
