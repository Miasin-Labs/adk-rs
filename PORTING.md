# ADK Python to Rust porting map

Upstream reference checkout: `/tmp/adk-python` at commit `065f4ae`.

## Core parity matrix

| ADK Python concept | Upstream file | Rust status |
| --- | --- | --- |
| Public API exports | `src/google/adk/__init__.py` | `src/lib.rs` re-exports core agent, event, runner, service, model, and tool types |
| `App` | `src/google/adk/apps/app.py` | `src/app.rs` ports app name, root agent, and plugin collection |
| `BaseAgent` / `LlmAgent` | `src/google/adk/agents/base_agent.py`, `src/google/adk/agents/llm_agent.py` | `src/agent.rs` ports a typed agent tree with model, instruction, tools, and sub-agents |
| `SequentialAgent` / workflow agents | `src/google/adk/agents/sequential_agent.py`, `src/google/adk/agents/parallel_agent.py`, `src/google/adk/agents/loop_agent.py` | `src/agent.rs` ports explicit `AgentKind` metadata for LLM, sequential, parallel, and loop agents |
| `Runner` | `src/google/adk/runners.py` | `src/runner.rs` ports session loading, user event append, iterative model/tool loop, plugin hooks, event append, and transfer execution against the agent tree |
| `InvocationContext` | `src/google/adk/agents/invocation_context.py` | `src/invocation.rs` ports app/user/session/invocation identity and LLM-call limit enforcement |
| `RunConfig` | `src/google/adk/agents/run_config.py` | `src/run_config.rs` ports streaming mode and max LLM calls |
| `Event` | `src/google/adk/events/event.py` | `src/event.rs` ports event id, invocation id, author, parts, actions, and timestamp field |
| `EventActions` | `src/google/adk/events/event_actions.py` | `src/event.rs` ports state/artifact deltas, transfer, escalation, compaction, tool confirmation/auth request placeholders, routing, widgets, and structured response |
| `Session` / `BaseSessionService` | `src/google/adk/sessions/session.py`, `src/google/adk/sessions/base_session_service.py` | `src/session.rs` ports app/user/session ids, state, events, append-state mutation, and in-memory store |
| `BaseArtifactService` | `src/google/adk/artifacts/base_artifact_service.py` | `src/artifact.rs` ports app/user/session-scoped versioned save/load/latest/list/delete |
| `BaseMemoryService` | `src/google/adk/memory/base_memory_service.py` | `src/memory.rs` ports add-memory, add-events, add-session, and search |
| `BasePlugin` | `src/google/adk/plugins/base_plugin.py` | `src/plugin.rs` and `src/runner/plugins.rs` port user/run/event/model/tool callback hooks |
| `BaseTool` / function tools | `src/google/adk/tools/base_tool.py`, `src/google/adk/tools/function_tool.py` | `src/tool.rs` ports tool spec/call/result and async trait boundary |
| `BaseLlm` / LLM request/response | `src/google/adk/models/` | `src/model.rs` ports provider-agnostic request/response and async language-model trait |
| Model adapters/catalog | `src/google/adk/models/anthropic_llm.py`, `google_llm.py`, `lite_llm.py`, `gemma_llm.py`, `registry.py` | `src/model.rs` ports provider/capability *metadata* (`ModelSpec`) for Gemini, Vertex AI, Anthropic, OpenAI-compatible, LiteLLM, Apigee, Gemma, and custom. The only *working* adapter is `OpenAiCompatibleModel` (`openai_compatible.rs`); native Gemini/Anthropic adapters are not yet implemented. |
| Tool catalog | `src/google/adk/tools/` | `src/tool.rs` ports built-in tool-kind coverage and registry/spec lookup for agent, API Hub, app integration, authenticated function, Bash, BigQuery, Bigtable, computer use, data-agent, search, OpenAPI, MCP, retrieval, Pub/Sub, Spanner, toolbox, transfer, URL context, Vertex AI search, memory/artifact tools, and more |
| Auth / credentials | `src/google/adk/auth/` | `src/auth.rs` ports auth schemes, credentials, and user/app-scoped credential service |
| A2A | `src/google/adk/a2a/` | `src/a2a.rs` ports agent cards, messages, remote agent wrapper, and transport trait |
| Code executors | `src/google/adk/code_executors/` | `src/code_executor.rs` ports code blocks, execution results, and executor trait |
| Planners | `src/google/adk/planners/` | `src/planner.rs` ports plan/step models and planner trait |
| Evaluation | `src/google/adk/evaluation/` | `src/eval.rs` ports eval cases, metrics/results, and in-memory eval service |
| Live streaming | `src/google/adk/agents/live_request_queue.py`, live runner paths | `src/live.rs` ports live request/response and FIFO queue |
| Workflow graph | `src/google/adk/workflow/` | `src/workflow.rs` ports node/edge graph, roots, next-node lookup, and node validation |
| Telemetry | `src/google/adk/telemetry/` | `src/telemetry.rs` ports spans, token usage, and in-memory telemetry sink |
| Integrations | `src/google/adk/integrations/` | `src/integration.rs` ports typed integration endpoint registry |
| Skills | `src/google/adk/skills/` | `src/skills.rs` ports skill model and registry |
| Optimization | `src/google/adk/optimization/` | `src/optimization.rs` ports prompt optimization candidate and optimizer trait |
| Environment/platform | `src/google/adk/environment/`, `platform/` | `src/environment.rs` and `src/platform.rs` port environment, clock, and UUID generation traits |
| CLI/API server shapes | `src/google/adk/cli/` | `src/cli.rs` and `src/server.rs` port command and route shapes for run, web, API server, eval, create, deploy, artifacts, sessions, SSE, and live |

## Parity status (audited)

The crate ports most ADK Python *types*, but many are **declaration-only**:
the trait/struct exists and is unit-tested, yet the `Runner` never invokes it.
The list below reflects what the execution loop actually drives, not just what
compiles. (Evidence: `Runner` imports only `agent`, `session`, `invocation`,
`tool`, `event`, `approval`, `model`, `guardrail`, `structured_output`,
`plugin`, `auth`, `run_config`, `run_trace`, `app`.)

### Functional (driven by the runtime)

- Typed agents and the agent tree (`agent.rs`).
- Workflow execution for all `AgentKind`s — `Sequential`, `Parallel`
  (isolated + concurrent), `Loop` (until escalate / max-iterations) — plus
  model-driven `transfer_to_agent` handoff (`runner.rs`).
- Tools, tool approval (suspend/resume), and the `HttpTool` (`tool.rs`,
  `runner/cycle.rs`, `http_tool.rs`).
- `OpenAiCompatibleModel` and `FallbackLanguageModel` (`openai_compatible.rs`,
  `fallback_model.rs`).
- Sessions (in-memory + file), artifacts (in-memory + file), structured
  output, guardrails, credential lookup, and a per-run memory **window**
  (event truncation, `run_config.memory_window_events`).

### Declaration-only (types exist; runtime does not call them yet)

- `MemoryService` retrieval — the runner truncates recent events but never
  searches/injects retrieved memory (RAG). `memory_window_events` is *not*
  `MemoryService`.
- `TelemetrySink` — no spans are emitted during a run.
- `Planner` (`build_plan`) — never invoked by the runner.
- `CodeExecutor` — trait only; no executor is wired in and no built-in impl
  ships.
- `WorkflowRuntime` — `run_from_roots` returns BFS visitation order only; it
  does not execute nodes (agent execution lives on `AgentKind`, not this graph).
- `A2A` (`RemoteA2aAgent`, `A2aTransport`) — types only, no live transport.
- Streaming / live — `StreamingResponseAggregator`, `LiveRequestQueue`, and
  `LiveMediaAdapter` exist, but `Runner` exposes only a non-streaming `run`;
  there is no `Runner::stream`.
- `Optimizer`, `SkillRegistry` (skills not injected into model context),
  `EvalService` / `metric` evaluators (not run post-turn), and
  `replay`/`Recording` (route shape only).

### Genuinely missing (no type yet)

- Native Gemini / Anthropic model adapters (only OpenAI-compatible exists).
- Encrypted / OS-keyring credential storage (files are plaintext JSON).
- Database or cloud session/artifact backends (in-memory + local file only).

## Frontend direction (intentional non-goal)

A web/dev UI is **deliberately out of scope** — the upstream ADK web app and
the earlier in-repo editor experiment were removed by choice. adk-rs is a
self-contained runtime driven through the core library, the `adk-cli`
inspection helper, and the `adk-mcp` MCP server (typed agents authored as JSON
or YAML specs). UI parity is not tracked as a gap.
