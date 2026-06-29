use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::{Event, EventAuthor, EventPart};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredOutputSchema {
    pub json_schema: Value,
}

impl StructuredOutputSchema {
    pub fn new(json_schema: Value) -> Self {
        Self { json_schema }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StructuredOutputError {
    #[error("structured output was requested but no final agent text was found")]
    MissingFinalText,
    #[error("structured output JSON parse failed: {0}")]
    InvalidJson(String),
    #[error("structured output must be a JSON object")]
    ExpectedObject,
    #[error("structured output is missing required field {0}")]
    MissingRequiredField(String),
}

pub fn parse_structured_output(
    events: &[Event],
    schema: Option<&StructuredOutputSchema>,
) -> Result<Option<Value>, StructuredOutputError> {
    let Some(schema) = schema else {
        return Ok(None);
    };
    let text = final_agent_text(events).ok_or(StructuredOutputError::MissingFinalText)?;
    let value = serde_json::from_str::<Value>(text)
        .map_err(|source| StructuredOutputError::InvalidJson(source.to_string()))?;
    validate_schema(&value, schema)?;
    Ok(Some(value))
}

fn final_agent_text(events: &[Event]) -> Option<&str> {
    events.iter().rev().find_map(|event| {
        if !matches!(event.author, EventAuthor::Agent(_)) {
            return None;
        }
        event.parts.iter().find_map(|part| match part {
            EventPart::Text(text) => Some(text.as_str()),
            EventPart::ToolCall(_) | EventPart::ToolResult(_) => None,
        })
    })
}

fn validate_schema(
    value: &Value,
    schema: &StructuredOutputSchema,
) -> Result<(), StructuredOutputError> {
    if schema.json_schema.get("type").and_then(Value::as_str) == Some("object")
        && !value.is_object()
    {
        return Err(StructuredOutputError::ExpectedObject);
    }
    if let Some(required) = schema.json_schema.get("required").and_then(Value::as_array) {
        let Some(object) = value.as_object() else {
            return Err(StructuredOutputError::ExpectedObject);
        };
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                return Err(StructuredOutputError::MissingRequiredField(
                    field.to_owned(),
                ));
            }
        }
    }
    Ok(())
}
