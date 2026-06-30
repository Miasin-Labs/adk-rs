use adk_rs::{ApiRoute, BuiltinToolKind, ModelRegistry, VisualAgentBuilder};
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
    ResolveModel {
        model: String,
    },
    DebugRun {
        prompt: String,
    },
    /// Work with typed agent spec files (JSON or YAML).
    Spec {
        #[command(subcommand)]
        action: SpecAction,
    },
}

#[derive(Debug, Subcommand)]
enum SpecAction {
    /// Parse and validate a `.json`/`.yaml`/`.yml` agent spec file.
    Validate {
        /// Path to the agent spec file.
        path: String,
    },
    /// Convert an agent spec file to another format on stdout.
    Convert {
        /// Path to the agent spec file.
        path: String,
        /// Output format: `json` (default) or `yaml`.
        #[arg(long, default_value = "json")]
        to: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.command {
        Command::Routes => print_json(route_paths())?,
        Command::Tools => print_json(tool_names())?,
        Command::ResolveModel { model } => print_json(ModelRegistry::resolve(&model))?,
        Command::DebugRun { prompt } => print_json(json!({ "response": prompt }))?,
        Command::Spec { action } => run_spec(action)?,
    }
    Ok(())
}

fn run_spec(action: SpecAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        SpecAction::Validate { path } => {
            let blueprint = VisualAgentBuilder::from_file(&path)?;
            println!("valid agent spec: {}", blueprint.name);
        }
        SpecAction::Convert { path, to } => {
            let blueprint = VisualAgentBuilder::from_file(&path)?;
            let out = match to.as_str() {
                "json" => VisualAgentBuilder::to_json(&blueprint)?,
                "yaml" | "yml" => VisualAgentBuilder::to_yaml(&blueprint)?,
                other => return Err(format!("unknown format '{other}'; use 'json' or 'yaml'").into()),
            };
            println!("{out}");
        }
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
