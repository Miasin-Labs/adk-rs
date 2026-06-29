use serde_json::{Value, json};

use super::events;
use super::state::DevUiState;
use super::tools::ToolObservation;

#[derive(Debug, Clone)]
pub(super) struct DevPendingApproval {
    pub session_id: String,
    pub invocation_id: String,
    pub tool_name: String,
    pub args: Value,
}

impl DevUiState {
    pub(super) async fn create_approval_event(
        &self,
        invocation_id: &str,
        session_id: &str,
        user_text: &str,
    ) -> Value {
        let approval_id = self.new_event_id();
        let args = json!({ "body": user_text, "channel": "email-preview" });
        let pending = DevPendingApproval {
            session_id: session_id.to_owned(),
            invocation_id: invocation_id.to_owned(),
            tool_name: "send_email".to_owned(),
            args: args.clone(),
        };
        self.pending_approvals
            .lock()
            .await
            .insert(approval_id.clone(), pending);
        events::approval_event(
            self,
            invocation_id,
            &approval_id,
            "send_email",
            "Approve sending this email?",
            args,
        )
    }

    pub async fn resume_approval(
        &self,
        session_id: &str,
        approval_id: &str,
        approved: bool,
    ) -> Vec<Value> {
        let pending = self.pending_approvals.lock().await.remove(approval_id);
        let Some(pending) = pending.filter(|pending| pending.session_id == session_id) else {
            return vec![events::error_event(
                self,
                approval_id,
                "Approval request was not found.",
            )];
        };
        let observation = ToolObservation {
            call_id: approval_id.to_owned(),
            name: pending.tool_name,
            args: pending.args,
            response: json!({
                "approved": approved,
                "sent": approved,
                "message": if approved { "Email sent" } else { "Email was not sent" }
            }),
            state_delta: Some(
                json!({ "lastApproval": { "id": approval_id, "approved": approved } }),
            ),
        };
        let mut events = vec![events::tool_response_event(
            self,
            &pending.invocation_id,
            &observation,
        )];
        events.push(events::agent_event(
            self,
            &pending.invocation_id,
            if approved {
                "Approved. I sent the email preview."
            } else {
                "Declined. I did not send the email."
            },
        ));
        self.persist_events(session_id, &events).await;
        events
    }
}

pub(super) fn approval_requested(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("send") && text.contains("email")
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[tokio::test]
    async fn approval_request_can_resume_normal() {
        let state = DevUiState::default();
        let session = state
            .create_session("hello_world".to_owned(), "user".to_owned(), None)
            .await;

        let events = state
            .run_events(&crate::dev_ui::types::RunAgentRequest {
                session_id: session.id.clone(),
                new_message: Some(json!({
                    "role": "user",
                    "parts": [{ "text": "send email with trail plan" }]
                })),
                state_delta: None,
            })
            .await;
        let approval_id = events[0]
            .get("actions")
            .and_then(|actions| actions.get("requestedToolConfirmations"))
            .and_then(Value::as_object)
            .and_then(|confirmations| confirmations.keys().next())
            .cloned()
            .expect("approval id");

        let resumed = state.resume_approval(&session.id, &approval_id, true).await;

        assert_eq!(
            resumed[0]["content"]["parts"][0]["functionResponse"]["response"]["sent"],
            true
        );
    }
}
