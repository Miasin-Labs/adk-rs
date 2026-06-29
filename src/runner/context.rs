use std::collections::BTreeMap;
use std::sync::Arc;

use crate::auth::CredentialService;
use crate::event::{Event, EventActions};
use crate::invocation::InvocationContext;
use crate::session::Session;
use crate::tool_context::{ReadonlyContext, ToolContext};

pub(super) fn request_events(events: &[Event], memory_window_events: Option<usize>) -> Vec<Event> {
    match memory_window_events {
        Some(window) => events
            .iter()
            .skip(events.len().saturating_sub(window))
            .cloned()
            .collect(),
        None => events.to_vec(),
    }
}

pub(super) fn tool_context(
    invocation: &InvocationContext,
    session: &Session,
    credential_service: Option<Arc<dyn CredentialService>>,
) -> ToolContext {
    ToolContext {
        readonly: ReadonlyContext {
            app_name: invocation.app_name.clone(),
            user_id: invocation.user_id.clone(),
            session_id: invocation.session_id.clone(),
            invocation_id: invocation.invocation_id.clone(),
            state: session
                .state
                .iter()
                .map(|(key, value)| (key.as_str().to_owned(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        },
        actions: EventActions::default(),
        credential_service,
    }
}
