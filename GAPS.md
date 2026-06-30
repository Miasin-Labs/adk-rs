# adk-rs Capability Gap Analysis

This document maps the class of beginner-friendly AI-agent workflows adk-rs aims
to support against the current `adk-rs` implementation. The goal is to make
`adk-rs` capable of building a scheduled or chat-triggered assistant — with a
brain, memory, tools, guardrails, and observable execution — entirely in Rust,
as a self-contained runtime.

## Target Capability

The concrete target is a morning trail-run advisor:

1. Runs every morning on a schedule.
2. Checks the user's calendar for a trail-run event.
3. Checks weather near the user.
4. Reads a saved trail list from a sheet-like data source.
5. Checks air quality through a custom HTTP API call.
6. Uses an LLM with memory and a structured prompt to decide what to recommend.
7. Sends a message or email with the recommendation.
8. Can also be driven through chat for ad hoc questions.
9. Has guardrails for risky actions, prompt injection, loops, and bad outputs.

The important product distinction is:

- Automation/workflow: fixed human-authored path.
- Agent: LLM decides the next action, uses tools, observes results, and
  iterates toward the goal.

`adk-rs` has the early runtime skeleton for the second shape, but it does not
yet have all of the surrounding builder, trigger, credential, and
production-control surfaces needed to recreate the full beginner-builder
experience.

## What adk-rs Already Has

- Agent composition: `src/agent.rs` has `Agent`, `AgentBuilder`, tools, and
  sub-agents.
- Runner loop: `src/runner.rs` records user events, calls the active model,
  executes requested tools, appends tool results, and follows
  `transfer_to_agent`.
- Tool boundary: `src/tool.rs` defines `Tool`, `ToolSpec`, `ToolCall`, and
  `ToolResult`.
- Sessions: `src/session.rs` has in-memory and file-backed session stores.
- Basic memory service: `src/memory.rs` stores and searches text entries.
- Credentials shape: `src/auth.rs` models API key, bearer, OAuth2, OIDC, and
  service-account credentials with in-memory and file-backed credential
  services. `AuthCredential` debug output is redacted.
- Hosted model adapter: `src/openai_compatible.rs` implements an
  OpenAI-compatible Chat Completions adapter with tool-schema posting and
  tool-call parsing.
- Model fallback: `src/fallback_model.rs` provides `FallbackLanguageModel` for
  ordered primary/backup model execution.
- Workflow graph: `src/workflow.rs` and `src/workflow_runtime.rs` define a
  static graph and traversal order.
- Visual sketching: `src/visual_builder.rs` parses a simple YAML agent tree and
  emits DOT.
- Typed agents over MCP: `crates/adk-mcp` creates, lists, runs, and persists
  typed `AgentSpec`s (specs authored in JSON or YAML), rebuilding each agent
  from its spec on every run.
- Typed prompt contract: `src/prompt.rs` provides `AgentPrompt` for role,
  task, input, tools, constraints, and output sections.
- Run controls and trace: `RunConfig` now includes `max_iterations` and
  `memory_window_events`; `RunOutput` includes `finish_reason`, `RunTrace`, and
  optional parsed structured output.
- Tool approval: tools can require approval and `Runner::resume_tool_call`
  resumes approved or declined pending calls.
- Generic HTTP tool foundation: `src/http_tool.rs` provides a basic `HttpTool`
  with GET/POST, query/header/body templates, optional static auth credential,
  credential-key lookup through `ToolContext`, domain allow-listing, and JSON
  response parsing.
- New examples: `examples/simple_agent.rs`, `examples/tool_agent.rs`,
  `examples/handoff_agents.rs`, `examples/react_agent.rs`, and
  `examples/trail_advisor.rs`.

## Reference Agent-Platform Capabilities

Mature agent platforms expose two relevant layers. They are useful as a
checklist for what a complete adk-rs runtime should eventually provide.

Programmatic agent SDK:

- An agent builder that accepts model, instructions, tools, deferred tools,
  provider tools, memory, middleware, guardrails, evals, checkpoints, structured
  output, concurrency, telemetry, MCP clients, and execution defaults.
- A tool model with input/output schemas, handler, model-output transformation,
  cancellation, approval, suspend/resume, provider options, and tool-specific
  system instructions.
- A memory model supporting in-process memory, persistent backends, episodic
  memory, observational memory, and thread title generation.
- An agent runtime loop with streaming, aborts, max iterations, tool-call
  repair, concurrent tool execution, suspend/resume, token/cost accounting,
  memory persistence, telemetry, and background tasks.
- Deferred tool loading (`search_tools` / `load_tool`) so large tool catalogs
  can be loaded on demand.
- Sub-agent delegation (`delegate_subagent`) with task-path tracking, max child
  limits, inline sub-agent models by difficulty, and parent/child accounting.

Visual workflow/node layer:

- An AI Agent node that requires a language-model input, accepts memory/tools/
  output-parser subnodes, and supports prompt source modes, fallback model,
  streaming hints, and builder hints.
- A model-callable HTTP tool node with method, URL, placeholders, auth,
  query/header/body parameters, and response optimization.
- A mature standalone HTTP request implementation with auth modes, redirect
  policy, timeout, proxy, request body modes, binary data, pagination, response
  format, and credential sanitization.
- Chat and schedule triggers as entry points.
- Guardrail nodes providing classify/sanitize flows with pass/fail outputs and
  optional LLM-backed checks.
- Authenticated chat execution with SSE, thread ownership checks, validation
  before run, resume, and message history.
- Persistence of agent drafts, published versions, tools, skills, integrations,
  executions, timelines, tool calls, token usage, cost, HITL state, and source.

## Gap Matrix

| Priority | Capability | adk-rs Today | Gap | Implementation Shape |
| --- | --- | --- | --- | --- |
| P0 | Real hosted model adapter | `OpenAiCompatibleModel` calls Chat Completions-compatible endpoints, sends tool schemas, and parses tool calls | Gemini/Anthropic-native adapters, streaming, structured output, usage/cost extraction, and credential-store constructor are still missing | Add provider-specific adapters or wrappers, usage parsing, streaming, and constructors that resolve credentials from `CredentialService` |
| P0 | Credential-backed tools and models | `ToolContext` exposes scoped credential lookup, `Runner` accepts a credential service, `HttpTool` can resolve `credential_key`, `FileCredentialService` persists credentials, and credential debug output is redacted | Credential files are plaintext JSON and model adapter construction still takes direct `AuthCredential` | Add encrypted/OS-keyring storage option, redaction helpers for logs/traces, and model constructors that resolve credential keys from `CredentialService` |
| P0 | HTTP request tool | Basic `HttpTool` exists with GET/POST, templates, credential-key lookup, optional static credential, allow-listing, and JSON response parsing | Still missing response optimization/truncation, pagination, retries, binary data, and mature error hints | Add timeout/retry/response-shaping options and keep tests on local fake HTTP servers |
| P0 | Prompt contract for agents | `AgentPrompt` renders role/task/input/tools/constraints/output into instructions | Prompt is not yet integrated into the visual builder/spec schema and does not validate required fields beyond caller usage | Add `AgentPrompt` support to visual builder schemas and examples; add prompt validation for runnable saved agents |
| P0 | Memory window | `Runner` can trim `ModelRequest.events` with `RunConfig.memory_window_events`; `MemoryService` stores searchable entries | Search/retrieval memory is still separate from the model loop | Add memory search injection into `ModelRequest`, persistent memory backends, and tests proving second-turn recall from memory service |
| P0 | Schedule trigger | No trigger runtime | Trail-run advisor cannot run every morning | Add a minimal `Trigger` trait and `ScheduleTrigger` config; CLI can run due jobs in-process first |
| P0 | Chat trigger/session surface | No stable chat API contract for agents | No reusable chat endpoint/loop for agent sessions | Add a chat entry point with session id, SSE/non-SSE, and message history; keep auth local-only until real auth exists |
| P0 | Observable execution timeline | `RunOutput` now includes `RunTrace` with model calls, tool calls, transfers, and final reason | Trace is not durable and lacks durations, errors, token/cost usage, and HITL state | Extend `RunTrace`, persist it in a file/SQLite execution store, and surface it through the CLI/MCP |
| P1 | Structured output parser | `RunConfig.structured_output_schema` parses final agent text into `RunOutput.structured_output` and validates required object fields | Validation is intentionally minimal and not a full JSON Schema implementation | Add complete JSON Schema validation or a typed deserializer layer for production use |
| P1 | Tool output transformation | `ToolResult.content` is sent as-is | Large or sensitive tool outputs go straight back to model | Add optional `Tool::to_model_output` or `ToolResult { raw, model_visible }` |
| P1 | Guardrails | `Guardrail` trait, phase enforcement, and built-in keyword, email/PII, and secret-token guardrails exist | No LLM-backed classification, sanitize mode, or pass/fail workflow branches yet | Add sanitize decisions, LLM-backed guardrails, and pass/fail outputs in the workflow graph |
| P1 | Human approval / suspend-resume | `ToolApprovalPolicy`, `FinishReason::Suspended`, `RunOutput.pending_approval`, and `Runner::resume_tool_call` exist | Pending approvals are in-memory only and do not survive process restart | Add durable pending-call persistence and reconnect approvals to persisted state after reload |
| P1 | Max iterations and loop finish reason | `RunConfig.max_iterations` and `FinishReason::{Stop, Transfer, MaxIterations}` exist | Finish reasons still do not cover error/suspend/tool-call terminal variants | Add error/suspended finish variants when approval/resume and structured error paths land |
| P1 | Streaming | `StreamingResponseAggregator` exists but runner examples are non-streaming | No first-class streaming model/tool event surface | Add `Runner::stream` returning typed chunks: agent start, model delta, tool start/end, final |
| P1 | Fallback model | `FallbackLanguageModel` tries ordered model adapters until one succeeds | No trace/telemetry attribution for which model failed or succeeded yet | Add fallback trace steps and per-model usage/error accounting |
| P1 | Tool catalog and deferred loading | `ToolRegistry` can register builtins, but runner exposes static agent tools | No dynamic tool discovery during a run | Add `ToolCatalog`, reserved `search_tools`/`load_tool`, and loaded-tool session state |
| P1 | Multi-agent manager delegation | `Agent` has static `sub_agents`; transfer just switches active agent | No task-specific delegation with result returned to parent | Add a `delegate_subagent` tool that runs a child agent with scoped context and returns a concise result |
| P1 | Visual builder parity | `src/visual_builder.rs` parses a YAML agent tree and emits DOT; `crates/adk-mcp` persists typed `AgentSpec`s | Builder schema is still shallow — no nodes, edges, subnode slots, credentials, or prompt mode | Extend the blueprint/spec schema to nodes, edges, subnode slots, credentials, and prompt mode |
| P1 | Workflow execution | `WorkflowRuntime` only returns traversal order | Graph is not executable | Add node executor registry and `WorkflowRuntime::execute` with data passing and node errors |
| P1 | Tool integrations for tutorial demo | `examples/trail_advisor.rs` demonstrates fake calendar/trail/message tools plus HTTP-backed weather and air-quality tools through the runner | Real Google/OpenWeather/AirNow/Gmail integrations are still absent | Keep the fake demo as CI-safe proof, then add real integrations behind credential-backed configs |
| P1 | Error assistance/debug surface | `RunTrace` records model/tool steps | Failures are not localized to a node/tool config | Add structured tool/node errors with `hint`, `retryable`, `credential_missing`, and `bad_parameter` classifications |
| P2 | Persistent agent drafts/publish history | `crates/adk-mcp` persists typed agent specs, but without draft/published versioning | No version history for saved agents | Add `AgentRepository` with draft/published versions and migration-friendly JSON/YAML |
| P2 | Execution storage and analytics | Telemetry sink stores spans/token usage in-memory | No durable execution history | Add file/SQLite execution store and CLI listing |
| P2 | MCP client/server tools | `crates/adk-mcp` is an MCP server; `ToolRegistry` has MCP-ish built-in kind names only | No working MCP *client* tool bridge | Add an MCP client tool adapter after HTTP and credentials |
| P2 | Runtime skills | `SkillRegistry` stores skills but runner does not load them | Skills are passive registry data | Add `list_skills`/`load_skill` tools and inject selected skill content into model context |
| P2 | Evaluation hooks | Evals are separate from runner | No automated post-run quality gate | Add `RunConfig.evals` and record eval results per execution |
| P2 | Cost accounting | `TelemetrySink` has token usage, not integrated with model adapters | No provider cost rollup | Add model pricing table and usage extraction in real adapters |

## Implementation Roadmap

### Phase 1: Make a real local agent useful

1. Add `OpenAiLanguageModel` with tool-call support and tests against a fake
   HTTP server. **Status: `OpenAiCompatibleModel` exists.**
2. Add file-backed credential storage and wire `CredentialManager` into model
   and tool execution. **Status: file-backed storage exists; `HttpTool` uses
   scoped credentials; model adapter accepts direct `AuthCredential`.**
3. Add `HttpTool` with GET/POST, JSON response handling, timeout, auth, and
   redaction.
4. Add memory-window injection into `Runner`.
5. Add `max_iterations` and explicit finish reasons.
6. Build a Rust `trail_advisor` example using fake calendar/weather/trail/email
   tools first, then swap in HTTP-backed tools. **Status: first fake/local
   milestone exists in `examples/trail_advisor.rs`.**

### Phase 2: Match the beginner-builder experience

1. Add `AgentPrompt` with role/task/input/tools/constraints/output sections.
2. Expand `VisualAgentBuilder` into a JSON/YAML graph with slots for brain,
   memory, tools, output parser, guardrails, and triggers.
3. Add `ScheduleTrigger` and a chat entry point.
4. Add `RunTrace` and node/tool error hints.
5. Surface agent config, trace, tool results, memory, and errors through the
   CLI and MCP server.

### Phase 3: Make agents safe enough for real workflows

1. Add guardrail trait and first built-ins: jailbreak keywords, secret-key
   detection, PII detection, and tool-call allow/deny.
   **Status: guardrail trait, phase enforcement, keyword, email/PII, and secret-token built-ins exist.**
2. Add approval/suspend/resume for send-email/calendar-write/database-write
   tools.
   **Status: in-memory approval suspend/resume exists. Durable store pending.**
3. Add structured output parser and fallback model wrapper.
   **Status: basic structured output and `FallbackLanguageModel` exist.**
4. Add persistent execution history with token/cost accounting.

### Phase 4: Scale beyond one agent

1. Add `delegate_subagent` as a normal tool with task name, goal, context,
   difficulty, and max-child policy.
2. Add dynamic tool catalog with `search_tools` and `load_tool`.
3. Wire `SkillRegistry` into `list_skills` and `load_skill` tools.
4. Add MCP client tools after the HTTP/credential layer is stable.

## First Concrete Milestone

The first milestone is a fully runnable, no-real-secrets trail advisor demo:

```bash
cargo run --example trail_advisor
```

It currently:

- Uses a scripted or fake HTTP model for deterministic CI tests.
- Reads fake calendar data.
- Reads fake weather and air-quality data through the same `HttpTool` path that
  real APIs will use later.
- Reads a local JSON trail list.
- Produces an email/message body but does not send it unless an approval flag is
  set.
- Emits a `RunTrace` showing each model/tool step.
- Configures a memory window for the run.

Still missing from the milestone: a second chat turn that proves memory recall
from previous trail context, and durable trace rendering through the CLI/MCP.

## Non-Goals for the First Pass

- Do not build a full no-code platform clone.
- Do not add real Google OAuth flows before generic credential injection and
  redaction are stable.
- Do not add dozens of integrations before the generic HTTP tool is reliable.
- Do not start with multi-agent delegation if one agent plus tools can satisfy
  the trail advisor workflow.
