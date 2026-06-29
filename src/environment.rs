use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub variables: BTreeMap<String, String>,
}

pub trait LocalEnvironment: Send + Sync {
    fn current(&self) -> Result<Environment, EnvironmentError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error("environment unavailable: {0}")]
    Unavailable(String),
}
