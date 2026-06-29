use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::ModelRequest;
use crate::tool_context::ToolContext;
use crate::tool_declaration::FunctionDeclaration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BuiltinToolKind {
    Agent,
    ApiHub,
    ApplicationIntegration,
    AuthenticatedFunction,
    Bash,
    BigQuery,
    Bigtable,
    ComputerUse,
    DataAgent,
    DiscoveryEngineSearch,
    EnterpriseSearch,
    Example,
    ExitLoop,
    Function,
    GetUserChoice,
    GoogleApi,
    GoogleMapsGrounding,
    GoogleSearch,
    GoogleSearchAgent,
    LangChain,
    LoadArtifacts,
    LoadMemory,
    LoadMcpResource,
    LoadWebPage,
    LongRunning,
    Mcp,
    OpenApi,
    PreloadMemory,
    PubSub,
    Retrieval,
    SetModelResponse,
    SkillToolset,
    Spanner,
    Toolbox,
    TransferToAgent,
    UrlContext,
    VertexAiSearch,
}

impl BuiltinToolKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Agent => "agent_tool",
            Self::ApiHub => "apihub_tool",
            Self::ApplicationIntegration => "application_integration_tool",
            Self::AuthenticatedFunction => "authenticated_function_tool",
            Self::Bash => "bash_tool",
            Self::BigQuery => "bigquery_tool",
            Self::Bigtable => "bigtable_tool",
            Self::ComputerUse => "computer_use_tool",
            Self::DataAgent => "data_agent_tool",
            Self::DiscoveryEngineSearch => "discovery_engine_search_tool",
            Self::EnterpriseSearch => "enterprise_search_tool",
            Self::Example => "example_tool",
            Self::ExitLoop => "exit_loop",
            Self::Function => "function_tool",
            Self::GetUserChoice => "get_user_choice",
            Self::GoogleApi => "google_api_tool",
            Self::GoogleMapsGrounding => "google_maps_grounding",
            Self::GoogleSearch => "google_search",
            Self::GoogleSearchAgent => "google_search_agent",
            Self::LangChain => "langchain_tool",
            Self::LoadArtifacts => "load_artifacts",
            Self::LoadMemory => "load_memory",
            Self::LoadMcpResource => "load_mcp_resource",
            Self::LoadWebPage => "load_web_page",
            Self::LongRunning => "long_running_tool",
            Self::Mcp => "mcp_toolset",
            Self::OpenApi => "openapi_toolset",
            Self::PreloadMemory => "preload_memory",
            Self::PubSub => "pubsub_tool",
            Self::Retrieval => "retrieval_tool",
            Self::SetModelResponse => "set_model_response",
            Self::SkillToolset => "skill_toolset",
            Self::Spanner => "spanner_tool",
            Self::Toolbox => "toolbox_toolset",
            Self::TransferToAgent => "transfer_to_agent",
            Self::UrlContext => "url_context",
            Self::VertexAiSearch => "vertex_ai_search",
        }
    }

    pub fn spec(self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_owned(),
            description: format!("ADK built-in {}", self.name()),
            input_schema: json!({ "type": "object" }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub content: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool {name} failed: {message}")]
    Failed { name: String, message: String },
    #[error("unknown tool {0}")]
    Unknown(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    fn is_long_running(&self) -> bool {
        false
    }

    fn defers_response(&self) -> bool {
        false
    }

    fn declaration(&self) -> Option<FunctionDeclaration> {
        Some(FunctionDeclaration::from_spec(&self.spec()))
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError>;

    async fn process_model_request(
        &self,
        _context: &mut ToolContext,
        _request: &mut ModelRequest,
    ) -> Result<(), ToolError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
    builtins: BTreeMap<String, BuiltinToolKind>,
}

impl ToolRegistry {
    pub fn with_all_builtin_specs() -> Self {
        let mut registry = Self::default();
        for &kind in ALL_BUILTINS {
            registry.builtins.insert(kind.name().to_owned(), kind);
        }
        registry
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.spec().name, tool);
    }

    pub fn spec(&self, name: &str) -> Option<ToolSpec> {
        self.tools
            .get(name)
            .map(|tool| tool.spec())
            .or_else(|| self.builtins.get(name).map(|kind| kind.spec()))
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = self
            .tools
            .values()
            .map(|tool| tool.spec())
            .collect::<Vec<_>>();
        specs.extend(self.builtins.values().map(|kind| kind.spec()));
        specs.sort_by(|left, right| left.name.cmp(&right.name));
        specs
    }
}

const ALL_BUILTINS: &[BuiltinToolKind] = &[
    BuiltinToolKind::Agent,
    BuiltinToolKind::ApiHub,
    BuiltinToolKind::ApplicationIntegration,
    BuiltinToolKind::AuthenticatedFunction,
    BuiltinToolKind::Bash,
    BuiltinToolKind::BigQuery,
    BuiltinToolKind::Bigtable,
    BuiltinToolKind::ComputerUse,
    BuiltinToolKind::DataAgent,
    BuiltinToolKind::DiscoveryEngineSearch,
    BuiltinToolKind::EnterpriseSearch,
    BuiltinToolKind::Example,
    BuiltinToolKind::ExitLoop,
    BuiltinToolKind::Function,
    BuiltinToolKind::GetUserChoice,
    BuiltinToolKind::GoogleApi,
    BuiltinToolKind::GoogleMapsGrounding,
    BuiltinToolKind::GoogleSearch,
    BuiltinToolKind::GoogleSearchAgent,
    BuiltinToolKind::LangChain,
    BuiltinToolKind::LoadArtifacts,
    BuiltinToolKind::LoadMemory,
    BuiltinToolKind::LoadMcpResource,
    BuiltinToolKind::LoadWebPage,
    BuiltinToolKind::LongRunning,
    BuiltinToolKind::Mcp,
    BuiltinToolKind::OpenApi,
    BuiltinToolKind::PreloadMemory,
    BuiltinToolKind::PubSub,
    BuiltinToolKind::Retrieval,
    BuiltinToolKind::SetModelResponse,
    BuiltinToolKind::SkillToolset,
    BuiltinToolKind::Spanner,
    BuiltinToolKind::Toolbox,
    BuiltinToolKind::TransferToAgent,
    BuiltinToolKind::UrlContext,
    BuiltinToolKind::VertexAiSearch,
];
