use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool::ToolSpec;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: Value,
    pub response_json_schema: Option<Value>,
}

impl FunctionDeclaration {
    pub fn from_spec(spec: &ToolSpec) -> Self {
        Self {
            name: spec.name.clone(),
            description: spec.description.clone(),
            parameters_json_schema: spec.input_schema.clone(),
            response_json_schema: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub args: ToolArgsConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolArgsConfig {
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolConfirmation {
    pub hint: String,
    pub confirmed: bool,
    pub payload: Value,
}
