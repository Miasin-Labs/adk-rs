use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationEndpoint {
    pub name: String,
    pub kind: IntegrationKind,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrationKind {
    AgentRegistry,
    ApiRegistry,
    BigQuery,
    Firestore,
    Gcs,
    ParameterManager,
    SecretManager,
    Slack,
    SkillRegistry,
    Custom(String),
}

#[derive(Debug, Default, Clone)]
pub struct IntegrationRegistry {
    endpoints: BTreeMap<String, IntegrationEndpoint>,
}

impl IntegrationRegistry {
    pub fn register(&mut self, endpoint: IntegrationEndpoint) {
        self.endpoints.insert(endpoint.name.clone(), endpoint);
    }

    pub fn get(&self, name: &str) -> Option<&IntegrationEndpoint> {
        self.endpoints.get(name)
    }
}
