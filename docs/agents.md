# Agent cookbook

This project keeps the agent runtime small and explicit:

1. Implement `LanguageModel` for the model provider or test double.
2. Build one or more `Agent`s with `AgentBuilder`.
3. Attach `Tool` implementations when the model can request work.
4. Run the tree with `Runner` and a `SessionStore`.
5. Read the returned `Event`s to inspect user messages, tool results, agent
   messages, and handoffs.

## LLM vs workflow vs agent

An LLM app is the simplest shape: a prompt goes in and a response comes out.
That is useful, but the model cannot see private state or take action unless
the application provides that context.

An AI workflow adds a human-defined path. For example, always read calendar
data first, then call a weather API, then summarize the answer. This is
powerful, but the human still decides the path ahead of time.

An AI agent moves the decision point into the model. The model receives a goal,
chooses a tool, observes the result, chooses whether to call another tool, and
eventually emits a final answer. That reason-act-observe loop is the core shape
this crate models.

## Single agent

Use one agent when a task can be handled by one model policy.

```bash
cargo run --example simple_agent
```

`examples/simple_agent.rs` builds a `planner` agent with a scripted model. The
example shows the lowest-friction path through `AgentBuilder`, `Runner`, and
`InMemorySessionStore`.

## Tool-using agent

Use a tool when the model should request deterministic work from Rust code.

```bash
cargo run --example tool_agent
```

`examples/tool_agent.rs` attaches a `word_count` tool. The scripted model first
requests a tool call, the runner executes the tool, then the model sees the
`ToolResult` event and writes the final answer.

## ReAct-style agent

Use a ReAct-style loop when the model should reason, act through tools, observe
results, and then decide whether another iteration is needed.

```bash
cargo run --example react_agent
```

`examples/react_agent.rs` models a tiny social-post agent. It searches for
source material, observes the result, asks a critique tool to check the draft,
then emits the final post.

## Trail advisor

Use this as the first n8n-style milestone: one agent with local calendar/trail
tools, two HTTP-backed tools, scoped credentials, and a safe message preview.

```bash
cargo run --example trail_advisor
```

`examples/trail_advisor.rs` keeps all external services fake and local, but it
uses the same runner, `HttpTool`, credential service, and memory-window controls
that real integrations will use later.

## Handoff agents

Use sub-agents when one agent should route to another specialized policy.

```bash
cargo run --example handoff_agents
```

`examples/handoff_agents.rs` creates a `router` agent with a `support_specialist`
sub-agent. The router emits `transfer_to_agent`, and the runner continues the
same invocation with the specialist.

## Runtime concepts

- `Agent`: name, instruction, model, tools, and optional sub-agents.
- `LanguageModel`: async boundary that turns a `ModelRequest` into a
  `ModelResponse`.
- `Tool`: deterministic Rust function exposed to the model as a named callable.
- `Runner`: records events, calls models, executes tools, and follows handoffs.
- `SessionStore`: persistence boundary for sessions and event history.
- `Event`: observable output from the runtime, including user text, tool
  results, agent text, and actions.

## Provider adapters

The core crate does not hard-code a hosted model. A real adapter should:

1. Translate `ModelRequest` into the provider's request format.
2. Translate provider text, tool calls, and handoff instructions into
   `ModelResponse`.
3. Return `ModelError` for provider failures or unsupported capabilities.
4. Keep credentials and HTTP client setup outside the agent tree.
