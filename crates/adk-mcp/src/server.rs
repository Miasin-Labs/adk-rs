//! The MCP server: a stateful tool router that creates, lists, runs, and
//! deletes adk-rs agents. Live `Agent`s are never stored — each run rebuilds
//! model + AgentBuilder + Runner from the persisted spec.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use adk_rs::{
    Agent, AgentBuilder, AgentKind, AgentName, AuthCredential, EventAuthor, EventPart,
    InMemorySessionStore, InvocationId, LanguageModel, OpenAiCompatibleConfig,
    OpenAiCompatibleModel, RunConfig, RunOutput, Runner, SessionId, ToolRegistry,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::schemars::{self, JsonSchema};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{AgentKindSpec, AgentRegistry, AgentSpec, AgentSummary, now_secs};
use crate::tools::{EXECUTABLE_TOOLS, resolve_tool};

/// OpenAI-compatible model provider config, derived from the environment.
#[derive(Clone)]
pub struct ModelProvider {
    /// MUST include the `/v1` path; the model appends `chat/completions`.
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub catalog: Vec<String>,
}

impl ModelProvider {
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let default_model = std::env::var("OPENAI_MODEL")
            .or_else(|_| std::env::var("ADK_OPENAI_MODEL"))
            .unwrap_or_else(|_| "gpt-4o-mini".into());
        let catalog = std::env::var("ADK_MCP_MODELS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(|model| model.trim().to_owned())
                    .filter(|model| !model.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| {
                vec!["gpt-4o-mini".into(), "gpt-4o".into(), "gpt-4.1-mini".into()]
            });
        Self {
            base_url,
            api_key,
            default_model,
            catalog,
        }
    }
}

// ================= request structs (Deserialize + JsonSchema) =================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateAgentRequest {
    #[schemars(description = "Unique agent name (used as the registry key).")]
    pub name: String,
    #[schemars(description = "System prompt / instruction for the agent.")]
    pub instructions: String,
    #[schemars(description = "Model id, e.g. 'gpt-4o-mini'. Defaults to the server default.")]
    pub model: Option<String>,
    #[schemars(description = "Human-readable description of the agent.")]
    pub description: Option<String>,
    #[schemars(description = "Executable tool names to attach (see list_builtin_tools).")]
    pub tools: Option<Vec<String>>,
    #[schemars(description = "Workflow kind: llm (default), sequential, parallel, or loop.")]
    pub kind: Option<AgentKindSpec>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateAgentRequest {
    #[schemars(description = "Name of the agent to update.")]
    pub name: String,
    #[schemars(description = "Replace the instruction, if provided.")]
    pub instructions: Option<String>,
    #[schemars(description = "Replace the model id, if provided.")]
    pub model: Option<String>,
    #[schemars(description = "Replace the description, if provided.")]
    pub description: Option<String>,
    #[schemars(description = "Replace the attached tool list, if provided.")]
    pub tools: Option<Vec<String>>,
    #[schemars(description = "Replace the workflow kind, if provided.")]
    pub kind: Option<AgentKindSpec>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentNameRequest {
    #[schemars(description = "Name of the agent.")]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunAgentRequest {
    #[schemars(description = "Name of the agent to run.")]
    pub name: String,
    #[schemars(description = "User message / prompt to send to the agent.")]
    pub prompt: String,
    #[schemars(description = "Session id for multi-turn continuity. Defaults to 'default'.")]
    pub session_id: Option<String>,
}

// ================= response structs (Serialize + JsonSchema) =================

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListAgentsResponse {
    pub agents: Vec<AgentSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteResponse {
    pub name: String,
    pub deleted: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RunResponse {
    pub session_id: String,
    pub finish_reason: String,
    pub output: String,
    pub structured_output: Option<Value>,
    pub event_count: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListModelsResponse {
    pub base_url: String,
    pub default_model: String,
    pub models: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// true = adk-mcp can actually execute it; false = advertised-only adk-rs builtin.
    pub executable: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListToolsResponse {
    pub tools: Vec<ToolInfo>,
}

// ============================ the server ============================

#[derive(Clone)]
pub struct AdkMcp {
    registry: AgentRegistry,
    /// Cloning shares the inner `Arc<Mutex<..>>`, so sessions persist across runs.
    sessions: InMemorySessionStore,
    provider: ModelProvider,
    invocation_counter: Arc<AtomicU64>,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl AdkMcp {
    pub fn new(registry: AgentRegistry, provider: ModelProvider) -> Self {
        Self {
            registry,
            sessions: InMemorySessionStore::default(),
            provider,
            invocation_counter: Arc::new(AtomicU64::new(0)),
            tool_router: Self::tool_router(),
        }
    }

    // ---- shims (the smallest bridges over adk-rs construction) ----

    fn build_model(&self, model: &str) -> Result<Arc<dyn LanguageModel>, String> {
        let config = OpenAiCompatibleConfig {
            base_url: self.provider.base_url.clone(),
            model: model.to_owned(),
            credential: AuthCredential::ApiKey(self.provider.api_key.clone()),
        };
        let model =
            OpenAiCompatibleModel::new(config).map_err(|error| format!("model init failed: {error:?}"))?;
        Ok(Arc::new(model))
    }

    fn build_agent(&self, spec: &AgentSpec) -> Result<Agent, String> {
        let model = self.build_model(&spec.model)?;
        let name = AgentName::new(spec.name.clone()).map_err(|error| format!("bad agent name: {error}"))?;
        let mut builder = AgentBuilder::new(name, spec.instructions.clone(), model)
            .description(spec.description.clone())
            .kind(match &spec.kind {
                AgentKindSpec::Llm => AgentKind::Llm,
                AgentKindSpec::Sequential => AgentKind::Sequential,
                AgentKindSpec::Parallel => AgentKind::Parallel,
                AgentKindSpec::Loop { max_iterations } => AgentKind::Loop {
                    max_iterations: *max_iterations,
                },
            });
        for tool_name in &spec.tools {
            match resolve_tool(tool_name) {
                Some(tool) => builder = builder.tool(tool),
                None => return Err(format!("unknown executable tool '{tool_name}'")),
            }
        }
        builder.build().map_err(|error| format!("agent build failed: {error}"))
    }

    /// Final assistant text (`RunOutput` has no accessor; reverse-scan events).
    fn final_text(output: &RunOutput) -> String {
        output
            .events
            .iter()
            .rev()
            .filter(|event| matches!(event.author, EventAuthor::Agent(_)))
            .find_map(|event| {
                event.parts.iter().rev().find_map(|part| match part {
                    EventPart::Text(text) => Some(text.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default()
    }

    // ------------------------------ tools ------------------------------

    #[tool(description = "Create a new agent from a name, instructions, optional model, tools and kind.")]
    pub async fn create_agent(
        &self,
        Parameters(req): Parameters<CreateAgentRequest>,
    ) -> Result<Json<AgentSpec>, String> {
        if req.name.trim().is_empty() {
            return Err("agent name must be non-empty".into());
        }
        if self.registry.get(&req.name).await.is_some() {
            return Err(format!("agent '{}' already exists; use update_agent", req.name));
        }
        let now = now_secs();
        let spec = AgentSpec {
            name: req.name,
            instructions: req.instructions,
            model: req.model.unwrap_or_else(|| self.provider.default_model.clone()),
            description: req.description.unwrap_or_default(),
            tools: req.tools.unwrap_or_default(),
            kind: req.kind.unwrap_or_default(),
            created_at: now,
            updated_at: now,
        };
        // Validate it actually builds before persisting.
        self.build_agent(&spec)?;
        self.registry.put(spec).await.map(Json).map_err(|error| error.to_string())
    }

    #[tool(description = "Update fields of an existing agent. Only provided fields change.")]
    pub async fn update_agent(
        &self,
        Parameters(req): Parameters<UpdateAgentRequest>,
    ) -> Result<Json<AgentSpec>, String> {
        let mut spec = self
            .registry
            .get(&req.name)
            .await
            .ok_or_else(|| format!("no agent named '{}'", req.name))?;
        if let Some(value) = req.instructions {
            spec.instructions = value;
        }
        if let Some(value) = req.model {
            spec.model = value;
        }
        if let Some(value) = req.description {
            spec.description = value;
        }
        if let Some(value) = req.tools {
            spec.tools = value;
        }
        if let Some(value) = req.kind {
            spec.kind = value;
        }
        spec.updated_at = now_secs();
        self.build_agent(&spec)?;
        self.registry.put(spec).await.map(Json).map_err(|error| error.to_string())
    }

    #[tool(description = "Get the full spec of a single agent by name.")]
    pub async fn get_agent(
        &self,
        Parameters(req): Parameters<AgentNameRequest>,
    ) -> Result<Json<AgentSpec>, String> {
        self.registry
            .get(&req.name)
            .await
            .map(Json)
            .ok_or_else(|| format!("no agent named '{}'", req.name))
    }

    #[tool(description = "List all registered agents (compact view).")]
    pub async fn list_agents(&self) -> Result<Json<ListAgentsResponse>, String> {
        Ok(Json(ListAgentsResponse {
            agents: self.registry.list().await,
        }))
    }

    #[tool(description = "Delete an agent by name.")]
    pub async fn delete_agent(
        &self,
        Parameters(req): Parameters<AgentNameRequest>,
    ) -> Result<Json<DeleteResponse>, String> {
        let deleted = self.registry.remove(&req.name).await.map_err(|error| error.to_string())?;
        Ok(Json(DeleteResponse {
            name: req.name,
            deleted,
        }))
    }

    #[tool(description = "Run an agent on a prompt and return its final text output.")]
    pub async fn run_agent(
        &self,
        Parameters(req): Parameters<RunAgentRequest>,
    ) -> Result<Json<RunResponse>, String> {
        let spec = self
            .registry
            .get(&req.name)
            .await
            .ok_or_else(|| format!("no agent named '{}'", req.name))?;
        if self.provider.api_key.is_empty() {
            return Err("OPENAI_API_KEY is not set; cannot run agents".into());
        }
        let agent = self.build_agent(&spec)?;
        let runner = Runner::new(self.sessions.clone(), agent).with_run_config(RunConfig::default());

        let session_id_str = req.session_id.unwrap_or_else(|| "default".into());
        let session_id =
            SessionId::new(session_id_str.clone()).map_err(|error| format!("bad session id: {error}"))?;
        let counter = self.invocation_counter.fetch_add(1, Ordering::Relaxed);
        let invocation =
            InvocationId::new(format!("inv-{counter}")).map_err(|error| format!("bad invocation id: {error}"))?;

        let output = runner
            .run(&session_id, invocation, req.prompt)
            .await
            .map_err(|error| format!("run failed: {error}"))?;

        Ok(Json(RunResponse {
            session_id: session_id_str,
            finish_reason: format!("{:?}", output.finish_reason),
            output: Self::final_text(&output),
            structured_output: output.structured_output.clone(),
            event_count: output.events.len(),
        }))
    }

    #[tool(description = "List model ids available to use when creating agents.")]
    pub async fn list_models(&self) -> Result<Json<ListModelsResponse>, String> {
        Ok(Json(ListModelsResponse {
            base_url: self.provider.base_url.clone(),
            default_model: self.provider.default_model.clone(),
            models: self.provider.catalog.clone(),
        }))
    }

    #[tool(description = "List tool names attachable to agents (executable + advertised-only builtins).")]
    pub async fn list_builtin_tools(&self) -> Result<Json<ListToolsResponse>, String> {
        let mut tools: Vec<ToolInfo> = EXECUTABLE_TOOLS
            .iter()
            .map(|(name, description)| ToolInfo {
                name: (*name).to_owned(),
                description: (*description).to_owned(),
                executable: true,
            })
            .collect();
        // adk-rs builtin specs are declaration-only (advertised, not executed here).
        for spec in ToolRegistry::with_all_builtin_specs().specs() {
            tools.push(ToolInfo {
                name: spec.name,
                description: spec.description,
                executable: false,
            });
        }
        Ok(Json(ListToolsResponse { tools }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AdkMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("adk-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Create, list, inspect, run, and delete adk-rs agents. \
                 Workflow: list_models / list_builtin_tools to discover options, \
                 create_agent to make one, run_agent to execute it, update_agent to iterate. \
                 Agents persist to disk and are rebuilt from spec on each run. \
                 Set OPENAI_API_KEY (and optionally OPENAI_BASE_URL, OPENAI_MODEL) to run them."
                    .to_owned(),
            )
    }
}
