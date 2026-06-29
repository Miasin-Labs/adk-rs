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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentSpec {
    /// Unique agent name (also the registry key and adk-rs `AgentName`).
    pub name: String,
    /// System prompt / instruction for the agent.
    pub instructions: String,
    /// Model id sent to the OpenAI-compatible backend, e.g. "gpt-4o-mini".
    pub model: String,
    /// Human-readable description.
    pub description: String,
    /// Names of executable tools (see `list_builtin_tools`) to attach.
    pub tools: Vec<String>,
    /// Agent workflow kind.
    pub kind: AgentKindSpec,
    /// Unix-epoch seconds of creation.
    pub created_at: u64,
    /// Unix-epoch seconds of last update.
    pub updated_at: u64,
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
