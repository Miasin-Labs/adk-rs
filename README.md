# adk-rs

`adk-rs` is an early Rust port of the core Google ADK ideas: typed agents, model
requests and responses, tool calls, sessions, artifacts, memory, metrics,
workflow graphs, and a small local dev server.

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
- `crates/adk-server`: a small Axum dev server that serves the real n8n editor
  UI wired to a Rust `/rest` compatibility surface (see `docs/n8n-ui.md`).
- `third_party/n8n-editor-ui`: copied n8n editor UI source with upstream license files.
- `examples/`: simple runnable agents that do not need network credentials.
- `docs/agents.md`: an agent cookbook with the main runtime concepts.
- `PORTING.md`: parity notes against the Python ADK reference.

## Quick start

```bash
cargo test
cargo run -p adk-cli -- routes
cargo run -p adk-cli -- tools
cargo run -p adk-server -- --port 8091
```

Then open the dev server at `http://127.0.0.1:8091`.

The dev server is for local development only. It is unauthenticated, uses
permissive CORS, and may read local provider credentials such as
`OPENAI_API_KEY`; bind it only to a trusted loopback interface and do not expose
it on a public network.

The dev server serves the **real, verbatim n8n editor UI** (the upstream Vue
SPA), wired to a Rust implementation of n8n's `/rest` contract. You can build
agent workflows on the n8n canvas — a curated node catalog (ADK Agent,
Sub-Agent, HTTP Tool, IF, Set, Code, Merge, Memory, Wait), JS expressions,
credentials, and run/resume with live canvas animation — all driven by the
adk-rs runner. See [`docs/n8n-ui.md`](docs/n8n-ui.md) for the architecture and
how to rebuild the UI.

## Run the example agents

```bash
cargo run --example simple_agent
cargo run --example tool_agent
cargo run --example handoff_agents
cargo run --example react_agent
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

## Example map

- `simple_agent`: one model response, closest to a basic LLM app.
- `tool_agent`: model asks Rust for deterministic work, then answers.
- `handoff_agents`: router agent transfers work to a specialist sub-agent.
- `react_agent`: reason-act-observe loop over search and critique tools.
- `trail_advisor`: n8n-style personal assistant demo with local HTTP tools,
  scoped credentials, memory-window config, and message-preview safety.

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
The core typed surfaces are present, including agents, sessions, tools, model
requests, events, memory, artifacts, evals, telemetry, live request queues,
workflow graphs, CLI/server shapes, and a local dev UI shell. See `PORTING.md`
for the remaining parity gaps.
