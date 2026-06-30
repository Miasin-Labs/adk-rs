# adk-rs

`adk-rs` is an early Rust port of the core Google ADK ideas: typed agents, model
requests and responses, tool calls, sessions, artifacts, memory, metrics,
workflow graphs, and an MCP server for creating and running typed agents.

The crate is provider-agnostic. You plug in a `LanguageModel` implementation,
compose `Agent`s with `AgentBuilder`, attach `Tool`s when needed, and run the
agent through a `Runner` backed by a `SessionStore`.

## Beginner mental model

The easiest way to understand this repo is the three-level ladder:

1. **LLM:** user input goes in, model output comes back.
2. **Workflow:** a human-coded path tells the model which tools to call and in
   what order.
3. **Agent:** the model receives a goal, chooses tools, observes interim
   results, and decides whether to keep iterating.

`adk-rs` is built around level three. The `Runner` owns the mechanical loop:
record the user message, call the active model, execute requested tools, append
tool results to the session, and let the model decide the next step.

## What is here

- `src/`: the core Rust library.
- `crates/adk-cli`: local CLI helpers for route, tool, and model inspection.
- `crates/adk-mcp`: an MCP server that creates, runs, and persists typed agents
  (specs authored in JSON or YAML) for any MCP client.
- `examples/`: simple runnable agents that do not need network credentials.
- `docs/agents.md`: an agent cookbook with the main runtime concepts.
- `PORTING.md`: parity notes against the Python ADK reference.

## Quick start

```bash
cargo test
cargo run -p adk-cli -- routes
cargo run -p adk-cli -- tools
cargo run -p adk-cli -- spec validate agent.yaml
```

## Typed agents from JSON or YAML

adk-rs agents are typed specs you can author by hand in JSON or YAML. There are
two complementary shapes:

- **`AgentSpec`** (`crates/adk-mcp`): the runnable registry record — name,
  instructions, model, tools, and workflow kind. Persisted to disk and rebuilt
  on every run.
- **`AgentBlueprint`** (`src/visual_builder.rs`): a recursive design tree
  (an agent plus nested `sub_agents`) used for sketching and graph export.

Both load from `.json`, `.yaml`, or `.yml`; only the identifying fields are
required and the rest default. A minimal `AgentSpec`:

```yaml
name: research
instructions: Answer with sources and show your working.
model: gpt-4o-mini          # optional; falls back to the server default
tools: [http_request, calculator]
kind:
  type: llm                  # llm (default) | sequential | parallel | loop
```

### Structured output

Add an optional `output_schema` (a JSON Schema) to an `AgentSpec` to make an
agent return validated JSON. When set, `run_agent` parses the agent's final
reply as JSON, checks it against the schema, and returns the parsed value as
`structured_output` (see `examples/agents/kalamazoo_picker.yaml`):

```yaml
name: kalamazoo_picker
instructions: |
  Reply with ONLY a JSON object: {"best_day": <ISO date>, "activities": [<string>, ...]}.
model: gpt-4o
tools: [http_request]
output_schema:
  type: object
  required: [best_day, activities]
```

### Over MCP

`crates/adk-mcp` is a standalone MCP server that lets any MCP client create,
list, run, export, and delete adk-rs agents. Relevant tools:

- `create_agent` — create from individual fields.
- `create_agent_from_spec` — create from a JSON/YAML spec document (format
  auto-detected).
- `create_agent_from_file` — create from a local `.json`/`.yaml`/`.yml` file.
- `export_agent` — dump an existing agent back out as JSON or YAML.

Set `OPENAI_API_KEY` (and optionally `OPENAI_BASE_URL`, `OPENAI_MODEL`) to run
agents.

### From the CLI

```bash
cargo run -p adk-cli -- spec validate agent.yaml          # parse + validate
cargo run -p adk-cli -- spec convert agent.yaml --to json # convert formats
```

> Note: YAML support uses `serde_yaml` 0.9, which is no longer actively
> maintained upstream. It is stable and fine for spec files today; revisit if a
> maintained YAML crate is needed later.

## Run the example agents

```bash
cargo run --example simple_agent
cargo run --example tool_agent
cargo run --example handoff_agents
cargo run --example react_agent
cargo run --example sequential_workflow
cargo run --example parallel_workflow
cargo run --example loop_workflow
cargo run --example trail_advisor
```

These examples use small scripted models so the runtime can be tested without an
API key. Replace those scripted models with a real provider adapter when wiring
the crate into an application.

## Minimal agent shape

```rust
use std::sync::Arc;

use adk_rs::{
    AgentBuilder, AgentName, InMemorySessionStore, InvocationId, LanguageModel, Runner, SessionId,
};

async fn demo(model: Arc<dyn LanguageModel>) -> Result<(), Box<dyn std::error::Error>> {
    let agent = AgentBuilder::new(
        AgentName::new("assistant")?,
        "Answer clearly and keep state in the session.",
        model,
    )
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), agent);
    let output = runner
        .run(
            &SessionId::new("demo-session")?,
            InvocationId::new("turn-1")?,
            "Draft a release checklist",
        )
        .await?;
    let _ = output;
    Ok(())
}
```

The runner records the user event, asks the active agent's model for a response,
executes requested tools, appends tool results, and follows `transfer_to_agent`
actions when the model hands off to a sub-agent.

## Workflow agents

An agent's `AgentKind` controls orchestration. All workflow kinds run their
`sub_agents` over the one shared session:

- **`Llm`** (default): a single agent that may hand off to a sub-agent when its
  model emits a `transfer_to_agent` action (see `handoff_agents`).
- **`Sequential`**: runs the `sub_agents` in declaration order, so each stage
  sees the previous stages' output (see `sequential_workflow`).
- **`Parallel`**: runs each sub-agent as an independent branch with no data
  dependency between branches, fanning their results back into the session.
- **`Loop { max_iterations }`**: re-runs the sub-agent pipeline until a child
  emits an `escalate` action or `max_iterations` is reached.

Build these with `AgentBuilder::sequential()` / `.parallel()` /
`.loop_agent(n)` plus `.sub_agent(..)` calls. A workflow kind with no
sub-agents degrades to a single LLM cycle, and a model-driven
`transfer_to_agent` still takes precedence within any stage. (The flat
`AgentSpec` over MCP describes a single agent, so author multi-stage pipelines
through the library or `AgentBlueprint`.)

## Example map

- `simple_agent`: one model response, closest to a basic LLM app.
- `tool_agent`: model asks Rust for deterministic work, then answers.
- `handoff_agents`: router agent transfers work to a specialist sub-agent.
- `react_agent`: reason-act-observe loop over search and critique tools.
- `sequential_workflow`: a `Sequential` agent runs its sub-agents in order
  (scope -> analyze -> report) over one shared session, each stage building on
  the last.
- `parallel_workflow`: a `Parallel` agent fans isolated, concurrent branches
  out (independent risk dimensions) and merges their results back in.
- `loop_workflow`: a `Loop` agent refines a draft until a child escalates
  (the evaluator-optimizer shape).
- `trail_advisor`: personal-assistant demo with local HTTP tools, scoped
  credentials, memory-window config, and message-preview safety.

## Real model adapters

`OpenAiCompatibleModel` provides the first hosted-model adapter shape. It posts
Chat Completions-compatible requests, sends tool schemas, parses tool calls, and
accepts `AuthCredential` values for authorization. Use a local fake server in
tests, then point `OpenAiCompatibleConfig.base_url` at an OpenAI-compatible
endpoint for live use.

For resilience, wrap multiple model adapters in `FallbackLanguageModel`; it will
try each model in order until one returns a response.

## Structured Output

Set `RunConfig.structured_output_schema` when an agent must return JSON. The
runner parses the final agent text into `RunOutput.structured_output` and
checks required object fields from the supplied JSON schema.

## Tool Approval

Tools can require approval by returning `ToolApprovalPolicy::Required`. The
runner then returns `FinishReason::Suspended` with `RunOutput.pending_approval`
instead of executing the tool. Call `Runner::resume_tool_call` with
`ResumeDecision::Approved` or `ResumeDecision::Declined` to continue.

## Current status

This repository is a working Rust foundation, not a complete Python ADK clone.
What the runtime actually *executes* today: typed agents; `Sequential`,
`Parallel`, and `Loop` workflow agents plus model-driven handoff; tools and
tool approval; the OpenAI-compatible model adapter with fallback; sessions and
artifacts (in-memory + file); structured output; and guardrails.

Many other ADK surfaces are present as typed shapes but are **declaration-only**
— defined and unit-tested, yet not yet invoked by the `Runner` (e.g.
`MemoryService` retrieval, `TelemetrySink`, `Planner`, `CodeExecutor`,
`WorkflowRuntime` node execution, A2A, and streaming/live). A web UI is an
intentional non-goal. See `PORTING.md` for the audited functional /
declaration-only / missing breakdown.
