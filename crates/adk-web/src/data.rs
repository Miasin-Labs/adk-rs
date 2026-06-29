use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Info,
    State,
    Artifacts,
    Evals,
}

impl DetailTab {
    pub const ALL: [Self; 4] = [Self::Info, Self::State, Self::Artifacts, Self::Evals];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::State => "State",
            Self::Artifacts => "Artifacts",
            Self::Evals => "Evals",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    User,
    Agent,
    ToolCall,
    ToolResponse,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEntry {
    pub kind: EventKind,
    pub text: String,
    pub chips: Vec<String>,
}

impl EventEntry {
    pub fn user(text: &str) -> Self {
        Self {
            kind: EventKind::User,
            text: text.to_owned(),
            chips: Vec::new(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: EventKind::Error,
            text: text.into(),
            chips: Vec::new(),
        }
    }

    pub fn from_server(value: &Value) -> Self {
        let chips = state_chips(value);
        let part = value
            .get("content")
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .and_then(|parts| parts.first());
        if let Some(call) = part.and_then(|part| part.get("functionCall")) {
            return Self {
                kind: EventKind::ToolCall,
                text: tool_call_text(call),
                chips,
            };
        }
        if let Some(response) = part.and_then(|part| part.get("functionResponse")) {
            return Self {
                kind: EventKind::ToolResponse,
                text: response
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool_response")
                    .to_owned(),
                chips,
            };
        }
        Self {
            kind: EventKind::Agent,
            text: part
                .and_then(|part| part.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("Empty model response")
                .to_owned(),
            chips,
        }
    }
}

pub const BACKEND_BASE: &str = "http://127.0.0.1:8093";
pub const APP_NAME: &str = "hello_world";
pub const USER_ID: &str = "user";

fn tool_call_text(call: &Value) -> String {
    let name = call.get("name").and_then(Value::as_str).unwrap_or("tool");
    let args = call.get("args").unwrap_or(&Value::Null);
    match name {
        "roll_die" => format!("roll_die({})", number_arg(args, "sides").unwrap_or(6)),
        "check_prime" => format!("check_prime({})", list_arg(args, "nums")),
        _ => format!("{name}()"),
    }
}

fn state_chips(value: &Value) -> Vec<String> {
    value
        .get("actions")
        .and_then(|actions| actions.get("stateDelta"))
        .and_then(Value::as_object)
        .map(|state| {
            state
                .keys()
                .map(|key| key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|keys| !keys.is_empty())
        .map(|keys| vec![format!("State: {keys}")])
        .unwrap_or_default()
}

fn number_arg(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn list_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            let body = items
                .iter()
                .filter_map(Value::as_i64)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{body}]")
        })
        .unwrap_or_else(|| "[]".to_owned())
}
