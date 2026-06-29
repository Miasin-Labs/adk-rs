# adk-mcp

A stdio [MCP](https://modelcontextprotocol.io) server (built on the official
[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) SDK) that lets an MCP
client such as **Claude Code** create, run, and manage [adk-rs](../..) agents.

## Tools

| Tool | Purpose |
| --- | --- |
| `create_agent` | Make an agent (`name`, `instructions`, optional `model`, `tools`, `kind`). |
| `update_agent` | Change fields of an existing agent. |
| `get_agent` / `list_agents` | Inspect agents. |
| `delete_agent` | Remove an agent. |
| `run_agent` | Run an agent on a prompt (multi-turn via `session_id`); returns its output. |
| `list_models` | Model ids usable when creating agents. |
| `list_builtin_tools` | Tool names attachable to agents (executable vs advertised-only). |

Agents are persisted as JSON specs under `ADK_MCP_DATA_DIR`
(default `$HOME/.local/share/adk-mcp/agents.json`) and rebuilt from spec on each
run — no live agent state is held.

## Environment

- `OPENAI_API_KEY` — required for `run_agent` (create/list work without it).
- `OPENAI_BASE_URL` — default `https://api.openai.com/v1` (must include `/v1`).
- `OPENAI_MODEL` — default model (default `gpt-4o-mini`).
- `ADK_MCP_MODELS` — comma-separated catalog returned by `list_models`.
- `ADK_MCP_DATA_DIR` — where agent specs are stored.

## Register with Claude Code

```bash
cargo build -p adk-mcp --release   # → target/release/adk-mcp

claude mcp add adk --scope user \
  --env OPENAI_API_KEY=sk-... \
  --env OPENAI_BASE_URL=https://api.openai.com/v1 \
  --env OPENAI_MODEL=gpt-4o-mini \
  -- /ABS/PATH/adk-rs/target/release/adk-mcp
```

Then, in a Claude Code session: *"list adk models, then create an agent named
`researcher` that summarizes text, and run it on this paragraph."*

Inspect the raw protocol with:
`npx @modelcontextprotocol/inspector target/release/adk-mcp`
