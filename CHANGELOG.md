# Changelog

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
