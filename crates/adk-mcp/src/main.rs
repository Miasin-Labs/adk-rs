//! adk-mcp: a stdio MCP server exposing tools to create and run adk-rs agents.
//!
//! STDIO RULE: all logging goes to stderr — stdout is the JSON-RPC channel.

mod progress;
mod registry;
mod server;
mod tools;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use crate::registry::{AgentRegistry, default_data_dir};
use crate::server::{AdkMcp, ModelProvider};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let data_dir = default_data_dir();
    tracing::info!(?data_dir, "starting adk-mcp server");

    let registry = AgentRegistry::load(&data_dir)?;
    let provider = ModelProvider::from_env();
    if provider.api_key.is_empty() {
        tracing::warn!("OPENAI_API_KEY not set: create/list work, run_agent will error until set");
    }

    let service = AdkMcp::new(registry, provider)
        .serve(stdio())
        .await
        .inspect_err(|error| tracing::error!("serving error: {error:?}"))?;

    service.waiting().await?;
    Ok(())
}
