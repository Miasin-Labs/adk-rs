//! File-backed registry of agent *specs*. Live `Agent`s are never stored —
//! each run rebuilds model + AgentBuilder + Runner from the spec.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Workflow kind for the agent. Serde-friendly mirror of `adk_rs::AgentKind`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AgentKindSpec {
    #[default]
    Llm,
    Sequential,
    Parallel,
    Loop {
        max_iterations: u32,
    },
}

/// Persisted configuration of a single agent. Built and run on demand.
///
/// A spec can be authored by hand in JSON or YAML; only `name` and
/// `instructions` are required. Missing fields fall back to sensible defaults
/// (the registry stamps `created_at`/`updated_at`, and an empty `model` is
/// filled with the provider default at creation time).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentSpec {
    /// Unique agent name (also the registry key and adk-rs `AgentName`).
    pub name: String,
    /// System prompt / instruction for the agent.
    pub instructions: String,
    /// Model id sent to the OpenAI-compatible backend, e.g. "gpt-4o-mini".
    #[serde(default)]
    pub model: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Names of executable tools (see `list_builtin_tools`) to attach.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Agent workflow kind.
    #[serde(default)]
    pub kind: AgentKindSpec,
    /// Optional JSON Schema. When set, the agent's final reply is parsed as
    /// JSON and validated against this schema; the parsed value is returned as
    /// `run_agent`'s `structured_output`. Omit for plain free-text agents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Unix-epoch seconds of creation.
    #[serde(default)]
    pub created_at: u64,
    /// Unix-epoch seconds of last update.
    #[serde(default)]
    pub updated_at: u64,
}

/// Serialization format for an authored [`AgentSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecFormat {
    Json,
    Yaml,
}

impl SpecFormat {
    /// Detect the format from a file extension (`.json`, `.yaml`, `.yml`).
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => Some(Self::Json),
            Some("yaml") | Some("yml") => Some(Self::Yaml),
            _ => None,
        }
    }

    /// Best-effort detection from document text: a leading `{` means JSON,
    /// anything else is treated as YAML.
    pub fn detect(content: &str) -> Self {
        if content.trim_start().starts_with('{') {
            Self::Json
        } else {
            Self::Yaml
        }
    }
}

impl AgentSpec {
    /// Parse an agent spec from JSON or YAML text.
    pub fn parse(content: &str, format: SpecFormat) -> anyhow::Result<Self> {
        let spec = match format {
            SpecFormat::Json => serde_json::from_str::<AgentSpec>(content)?,
            SpecFormat::Yaml => serde_yaml::from_str::<AgentSpec>(content)?,
        };
        Ok(spec)
    }

    /// Load an agent spec from a `.json`, `.yaml`, or `.yml` file.
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let format = SpecFormat::from_path(path).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported spec extension for '{}' (use .json, .yaml, or .yml)",
                path.display()
            )
        })?;
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content, format)
    }

    /// Serialize the spec to pretty JSON.
    pub fn to_json_string(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Serialize the spec to YAML.
    pub fn to_yaml_string(&self) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}

/// Compact view returned by `list_agents`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AgentSummary {
    pub name: String,
    pub description: String,
    pub model: String,
    pub tool_count: usize,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Thread-safe, JSON-file-backed map of name -> AgentSpec.
#[derive(Clone)]
pub struct AgentRegistry {
    inner: Arc<Mutex<HashMap<String, AgentSpec>>>,
    path: PathBuf,
}

impl AgentRegistry {
    /// Load from `<data_dir>/agents.json`, creating the dir if needed.
    pub fn load(data_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("agents.json");
        let map: HashMap<String, AgentSpec> = if path.exists() {
            serde_json::from_slice(&std::fs::read(&path)?).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(map)),
            path,
        })
    }

    pub async fn list(&self) -> Vec<AgentSummary> {
        let map = self.inner.lock().await;
        let mut out: Vec<AgentSummary> = map
            .values()
            .map(|spec| AgentSummary {
                name: spec.name.clone(),
                description: spec.description.clone(),
                model: spec.model.clone(),
                tool_count: spec.tools.len(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub async fn get(&self, name: &str) -> Option<AgentSpec> {
        self.inner.lock().await.get(name).cloned()
    }

    /// Insert/replace and persist. Returns the stored spec.
    pub async fn put(&self, spec: AgentSpec) -> anyhow::Result<AgentSpec> {
        let mut map = self.inner.lock().await;
        map.insert(spec.name.clone(), spec.clone());
        Self::persist(&self.path, &map)?;
        Ok(spec)
    }

    pub async fn remove(&self, name: &str) -> anyhow::Result<bool> {
        let mut map = self.inner.lock().await;
        let existed = map.remove(name).is_some();
        if existed {
            Self::persist(&self.path, &map)?;
        }
        Ok(existed)
    }

    /// Atomic-ish write: serialize to a temp file then rename over the target.
    fn persist(path: &Path, map: &HashMap<String, AgentSpec>) -> anyhow::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(map)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// `ADK_MCP_DATA_DIR`, else `$HOME/.local/share/adk-mcp`, else `./`.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ADK_MCP_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/adk-mcp");
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AgentSpec {
        AgentSpec {
            name: "research".to_owned(),
            instructions: "Answer with sources.".to_owned(),
            model: "gpt-4o-mini".to_owned(),
            description: "research helper".to_owned(),
            tools: vec!["http_request".to_owned(), "calculator".to_owned()],
            kind: AgentKindSpec::Loop { max_iterations: 4 },
            output_schema: None,
            created_at: 100,
            updated_at: 200,
        }
    }

    #[test]
    fn json_and_yaml_parse_to_same_spec_normal() {
        let json = r#"
        {
          "name": "research",
          "instructions": "Answer with sources.",
          "model": "gpt-4o-mini",
          "description": "research helper",
          "tools": ["http_request", "calculator"],
          "kind": { "type": "loop", "max_iterations": 4 },
          "created_at": 100,
          "updated_at": 200
        }"#;
        let yaml = r#"
        name: research
        instructions: Answer with sources.
        model: gpt-4o-mini
        description: research helper
        tools: [http_request, calculator]
        kind:
          type: loop
          max_iterations: 4
        created_at: 100
        updated_at: 200
        "#;
        let from_json = AgentSpec::parse(json, SpecFormat::Json).unwrap();
        let from_yaml = AgentSpec::parse(yaml, SpecFormat::Yaml).unwrap();
        assert_eq!(from_json, from_yaml);
        assert_eq!(from_json, sample());
    }

    #[test]
    fn spec_round_trips_through_both_formats_normal() {
        let spec = sample();
        let json = spec.to_json_string().unwrap();
        let yaml = spec.to_yaml_string().unwrap();
        assert_eq!(AgentSpec::parse(&json, SpecFormat::Json).unwrap(), spec);
        assert_eq!(AgentSpec::parse(&yaml, SpecFormat::Yaml).unwrap(), spec);
    }

    #[test]
    fn minimal_spec_uses_defaults_normal() {
        let yaml = "name: tiny\ninstructions: Be brief.\n";
        let spec = AgentSpec::parse(yaml, SpecFormat::Yaml).unwrap();
        assert_eq!(spec.name, "tiny");
        assert_eq!(spec.instructions, "Be brief.");
        assert!(spec.model.is_empty());
        assert!(spec.tools.is_empty());
        assert_eq!(spec.kind, AgentKindSpec::Llm);
        assert!(spec.output_schema.is_none());
        assert_eq!(spec.created_at, 0);
    }

    #[test]
    fn output_schema_parses_and_round_trips_normal() {
        let yaml = r#"
        name: picker
        instructions: Reply with JSON.
        output_schema:
          type: object
          required: [best_day, activities]
        "#;
        let spec = AgentSpec::parse(yaml, SpecFormat::Yaml).unwrap();
        let schema = spec.output_schema.as_ref().expect("schema present");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "best_day");
        // Round-trips through both formats unchanged.
        let json = spec.to_json_string().unwrap();
        assert_eq!(AgentSpec::parse(&json, SpecFormat::Json).unwrap(), spec);
        let yaml_out = spec.to_yaml_string().unwrap();
        assert_eq!(AgentSpec::parse(&yaml_out, SpecFormat::Yaml).unwrap(), spec);
    }

    #[test]
    fn detect_format_from_content_normal() {
        assert_eq!(SpecFormat::detect("  { \"name\": \"x\" }"), SpecFormat::Json);
        assert_eq!(SpecFormat::detect("name: x"), SpecFormat::Yaml);
    }

    #[test]
    fn from_file_detects_format_by_extension_normal() {
        let base = std::env::temp_dir().join(format!(
            "adk-mcp-spec-{}-{}",
            std::process::id(),
            now_secs()
        ));
        std::fs::create_dir_all(&base).unwrap();

        let yaml_path = base.join("agent.yaml");
        std::fs::write(&yaml_path, sample().to_yaml_string().unwrap()).unwrap();
        assert_eq!(AgentSpec::from_file(&yaml_path).unwrap(), sample());

        let json_path = base.join("agent.json");
        std::fs::write(&json_path, sample().to_json_string().unwrap()).unwrap();
        assert_eq!(AgentSpec::from_file(&json_path).unwrap(), sample());

        let bad_path = base.join("agent.txt");
        std::fs::write(&bad_path, "name: x\ninstructions: y\n").unwrap();
        assert!(AgentSpec::from_file(&bad_path).is_err());

        let _ = std::fs::remove_dir_all(&base);
    }
}
