use std::collections::BTreeSet;
use std::path::Path;

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

/// Serialization format for an authored agent blueprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueprintFormat {
    Json,
    Yaml,
}

impl BlueprintFormat {
    /// Detect the format from a file extension (`.json`, `.yaml`, `.yml`).
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => Some(Self::Json),
            Some("yaml") | Some("yml") => Some(Self::Yaml),
            _ => None,
        }
    }
}

pub struct VisualAgentBuilder;

impl VisualAgentBuilder {
    /// Parse a YAML agent blueprint.
    pub fn parse_yaml(input: &str) -> Result<AgentBlueprint, VisualBuilderError> {
        let blueprint = serde_yaml::from_str::<AgentBlueprint>(input)
            .map_err(|source| VisualBuilderError::Yaml { source })?;
        validate_unique_names(&blueprint)?;
        Ok(blueprint)
    }

    /// Parse a JSON agent blueprint.
    pub fn parse_json(input: &str) -> Result<AgentBlueprint, VisualBuilderError> {
        let blueprint = serde_json::from_str::<AgentBlueprint>(input)
            .map_err(|source| VisualBuilderError::Json { source })?;
        validate_unique_names(&blueprint)?;
        Ok(blueprint)
    }

    /// Parse an agent blueprint from text in the given format.
    pub fn parse(input: &str, format: BlueprintFormat) -> Result<AgentBlueprint, VisualBuilderError> {
        match format {
            BlueprintFormat::Json => Self::parse_json(input),
            BlueprintFormat::Yaml => Self::parse_yaml(input),
        }
    }

    /// Load an agent blueprint from a `.json`, `.yaml`, or `.yml` file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<AgentBlueprint, VisualBuilderError> {
        let path = path.as_ref();
        let format = BlueprintFormat::from_path(path).ok_or_else(|| {
            VisualBuilderError::UnknownFormat {
                path: path.display().to_string(),
            }
        })?;
        let content =
            std::fs::read_to_string(path).map_err(|source| VisualBuilderError::Io {
                path: path.display().to_string(),
                source,
            })?;
        Self::parse(&content, format)
    }

    /// Serialize a blueprint to pretty JSON.
    pub fn to_json(blueprint: &AgentBlueprint) -> Result<String, VisualBuilderError> {
        serde_json::to_string_pretty(blueprint).map_err(|source| VisualBuilderError::Json { source })
    }

    /// Serialize a blueprint to YAML.
    pub fn to_yaml(blueprint: &AgentBlueprint) -> Result<String, VisualBuilderError> {
        serde_yaml::to_string(blueprint).map_err(|source| VisualBuilderError::Yaml { source })
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
    #[error("agent JSON is invalid")]
    Json { source: serde_json::Error },
    #[error("cannot read agent spec file '{path}'")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported spec file extension for '{path}' (use .json, .yaml, or .yml)")]
    UnknownFormat { path: String },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AgentBlueprint {
        AgentBlueprint {
            name: "router".to_owned(),
            model: "gpt-4o-mini".to_owned(),
            instruction: "Route the request.".to_owned(),
            tools: vec!["calculator".to_owned()],
            sub_agents: vec![AgentBlueprint {
                name: "worker".to_owned(),
                model: "gpt-4o-mini".to_owned(),
                instruction: "Do the work.".to_owned(),
                tools: vec![],
                sub_agents: vec![],
            }],
        }
    }

    #[test]
    fn json_and_yaml_parse_to_same_blueprint_normal() {
        let json = r#"
        {
          "name": "router",
          "model": "gpt-4o-mini",
          "instruction": "Route the request.",
          "tools": ["calculator"],
          "sub_agents": [
            { "name": "worker", "model": "gpt-4o-mini", "instruction": "Do the work." }
          ]
        }"#;
        let yaml = r#"
        name: router
        model: gpt-4o-mini
        instruction: Route the request.
        tools: [calculator]
        sub_agents:
          - name: worker
            model: gpt-4o-mini
            instruction: Do the work.
        "#;
        let from_json = VisualAgentBuilder::parse_json(json).unwrap();
        let from_yaml = VisualAgentBuilder::parse_yaml(yaml).unwrap();
        assert_eq!(from_json, from_yaml);
        assert_eq!(from_json, sample());
    }

    #[test]
    fn round_trips_through_both_formats_normal() {
        let bp = sample();
        let json = VisualAgentBuilder::to_json(&bp).unwrap();
        let yaml = VisualAgentBuilder::to_yaml(&bp).unwrap();
        assert_eq!(VisualAgentBuilder::parse_json(&json).unwrap(), bp);
        assert_eq!(VisualAgentBuilder::parse_yaml(&yaml).unwrap(), bp);
    }

    #[test]
    fn from_file_detects_format_by_extension_normal() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("agent.yaml");
        std::fs::write(&yaml_path, VisualAgentBuilder::to_yaml(&sample()).unwrap()).unwrap();
        assert_eq!(VisualAgentBuilder::from_file(&yaml_path).unwrap(), sample());

        let json_path = dir.path().join("agent.json");
        std::fs::write(&json_path, VisualAgentBuilder::to_json(&sample()).unwrap()).unwrap();
        assert_eq!(VisualAgentBuilder::from_file(&json_path).unwrap(), sample());
    }

    #[test]
    fn from_file_rejects_unknown_extension_robust() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.txt");
        std::fs::write(&path, "name: x\nmodel: m\ninstruction: i\n").unwrap();
        let err = VisualAgentBuilder::from_file(&path).unwrap_err();
        assert!(matches!(err, VisualBuilderError::UnknownFormat { .. }));
    }

    #[test]
    fn duplicate_sub_agent_name_is_rejected_robust() {
        let mut bp = sample();
        bp.sub_agents[0].name = "router".to_owned();
        let json = VisualAgentBuilder::to_json(&bp).unwrap();
        let err = VisualAgentBuilder::parse_json(&json).unwrap_err();
        assert!(matches!(err, VisualBuilderError::DuplicateAgentName(_)));
    }
}
