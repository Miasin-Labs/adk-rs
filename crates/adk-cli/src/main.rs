use adk_rs::{ApiRoute, BuiltinToolKind, ModelRegistry};
use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Debug, Parser)]
#[command(name = "adk", about = "ADK Rust local runtime tools")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Routes,
    Tools,
    ResolveModel { model: String },
    DebugRun { prompt: String },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.command {
        Command::Routes => print_json(route_paths())?,
        Command::Tools => print_json(tool_names())?,
        Command::ResolveModel { model } => print_json(ModelRegistry::resolve(&model))?,
        Command::DebugRun { prompt } => print_json(json!({ "response": prompt }))?,
    }
    Ok(())
}

fn route_paths() -> Vec<&'static str> {
    [
        ApiRoute::Apps,
        ApiRoute::Sessions,
        ApiRoute::Events,
        ApiRoute::Artifacts,
        ApiRoute::EvalSets,
        ApiRoute::Builder,
        ApiRoute::Recordings,
        ApiRoute::Metrics,
        ApiRoute::DeployPlan,
        ApiRoute::Run,
        ApiRoute::RunSse,
        ApiRoute::Live,
    ]
    .into_iter()
    .map(ApiRoute::path)
    .collect()
}

fn tool_names() -> Vec<String> {
    let registry = adk_rs::ToolRegistry::with_all_builtin_specs();
    registry.specs().into_iter().map(|spec| spec.name).collect()
}

fn print_json(value: impl serde::Serialize) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

const _: BuiltinToolKind = BuiltinToolKind::GoogleSearch;
