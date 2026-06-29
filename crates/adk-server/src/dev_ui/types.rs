use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSession {
    pub id: String,
    pub app_name: String,
    pub user_id: String,
    pub state: Value,
    pub events: Vec<Value>,
    pub last_update_time: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub session_id: Option<String>,
    pub state: Option<Value>,
    pub events: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionRequest {
    pub state_delta: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunAgentRequest {
    pub session_id: String,
    pub new_message: Option<Value>,
    pub state_delta: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeApprovalRequest {
    pub session_id: String,
    pub approved: bool,
}

#[derive(Debug, Deserialize)]
pub struct EvalSetCreateRequest {
    pub eval_set: Option<Value>,
}

pub fn parse_body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Option<T> {
    if body.is_empty() {
        return None;
    }
    serde_json::from_slice(body).ok()
}
