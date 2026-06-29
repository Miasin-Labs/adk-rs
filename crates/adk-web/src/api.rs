use gloo_net::http::Request;
use serde_json::{Value, json};

use crate::data::{APP_NAME, BACKEND_BASE, EventEntry, USER_ID};

pub async fn create_session() -> Result<String, String> {
    let response = Request::post(&format!(
        "{BACKEND_BASE}/apps/{APP_NAME}/users/{USER_ID}/sessions"
    ))
    .header("content-type", "application/json")
    .body("null")
    .map_err(|error| error.to_string())?
    .send()
    .await
    .map_err(|error| error.to_string())?;
    let session = response
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())?;
    session
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Session response did not include an id".to_owned())
}

pub async fn run_sse(session_id: &str, prompt: &str) -> Result<Vec<EventEntry>, String> {
    let body = json!({
        "appName": APP_NAME,
        "userId": USER_ID,
        "sessionId": session_id,
        "newMessage": { "role": "user", "parts": [{ "text": prompt }] },
        "streaming": false
    });
    let response = Request::post(&format!("{BACKEND_BASE}/run_sse"))
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(body.to_string())
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let text = response.text().await.map_err(|error| error.to_string())?;
    parse_sse(&text)
}

fn parse_sse(text: &str) -> Result<Vec<EventEntry>, String> {
    let mut events = Vec::new();
    for line in text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("data:"))
    {
        let value =
            serde_json::from_str::<Value>(line.trim()).map_err(|error| error.to_string())?;
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            events.push(EventEntry::error(error));
        } else {
            events.push(EventEntry::from_server(&value));
        }
    }
    Ok(events)
}
