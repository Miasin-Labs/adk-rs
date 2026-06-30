# Changelog

## Workflow agent execution (Sequential, Parallel, Loop)

The runner now acts on `AgentKind`. Previously the kind was stored but inert and
only model-driven `transfer_to_agent` handoffs ran. All workflow kinds run their
`sub_agents` over one shared session:

- **`Sequential`**: runs sub-agents in declaration order, so each stage builds
  on the previous stages' output.
- **`Parallel`**: runs each sub-agent as an independent branch, fanning results
  back into the session.
- **`Loop { max_iterations }`**: re-runs the sub-agent pipeline until a child
  emits an `escalate` action or `max_iterations` is reached.

Orchestration is recursive, so workflow agents nest. A workflow kind with no
sub-agents degrades to a single LLM cycle, and a model-driven
`transfer_to_agent` still takes precedence within a stage. Added the
`sequential_workflow` example and runner tests for all three kinds.

## Standalone runtime: dev-server / external editor UI removed

adk-rs is now a self-contained Rust agent runtime. The previous experiment that
embedded an external workflow-editor SPA and a Rust `/rest` compatibility server
has been removed in favor of typed agents driven directly from the library, the
CLI, and the MCP server.

### Removed
- The `adk-server` crate (the Axum dev server that hosted the external editor UI
  and its `/rest` boot/workflow/expression compatibility surface).
- The vendored editor-ui source tree and its build/staging script.
- The dev-server / editor-UI architecture note.

### Direction
- Agents are typed `AgentSpec`s created, run, and persisted through
  `crates/adk-mcp`, with specs authored in JSON or YAML.
- The core library (`src/`) keeps the typed agent, session, tool, memory,
  artifact, eval, telemetry, and workflow-graph surfaces unchanged.
