use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBlueprint {
    pub name: String,
    pub model: String,
    pub instruction: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub sub_agents: Vec<AgentBlueprint>,
}

pub struct VisualAgentBuilder;

impl VisualAgentBuilder {
    pub fn parse_yaml(input: &str) -> Result<AgentBlueprint, VisualBuilderError> {
        let blueprint = serde_yaml::from_str::<AgentBlueprint>(input)
            .map_err(|source| VisualBuilderError::Yaml { source })?;
        validate_unique_names(&blueprint)?;
        Ok(blueprint)
    }

    pub fn to_dot(blueprint: &AgentBlueprint) -> Result<String, VisualBuilderError> {
        validate_unique_names(blueprint)?;
        let mut dot = String::from("digraph adk_agent {\n  rankdir=LR;\n");
        write_node(blueprint, &mut dot);
        dot.push_str("}\n");
        Ok(dot)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VisualBuilderError {
    #[error("agent YAML is invalid")]
    Yaml { source: serde_yaml::Error },
    #[error("duplicate agent name {0}")]
    DuplicateAgentName(String),
}

fn validate_unique_names(blueprint: &AgentBlueprint) -> Result<(), VisualBuilderError> {
    let mut names = BTreeSet::new();
    collect_names(blueprint, &mut names)
}

fn collect_names(
    blueprint: &AgentBlueprint,
    names: &mut BTreeSet<String>,
) -> Result<(), VisualBuilderError> {
    if !names.insert(blueprint.name.clone()) {
        return Err(VisualBuilderError::DuplicateAgentName(
            blueprint.name.clone(),
        ));
    }
    for child in &blueprint.sub_agents {
        collect_names(child, names)?;
    }
    Ok(())
}

fn write_node(blueprint: &AgentBlueprint, dot: &mut String) {
    dot.push_str(&format!(
        "  {} [label=\"{}\\n{}\"];\n",
        blueprint.name, blueprint.name, blueprint.model
    ));
    for child in &blueprint.sub_agents {
        dot.push_str(&format!("  {} -> {};\n", blueprint.name, child.name));
        write_node(child, dot);
    }
}
