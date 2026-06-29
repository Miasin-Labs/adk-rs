# Dev UI: the real n8n editor

`adk-server` serves the **real, verbatim n8n editor-ui** (the upstream Vue 3
SPA), not a hand-written shell. The SPA is built from the upstream `n8n`
workspace and wired to a small n8n-compatible REST surface implemented in Rust
so it boots straight to its canvas with no signin/setup wall.

## Layout

| Path | What |
| --- | --- |
| `n8n/` | Upstream `n8n-io/n8n` clone (gitignored). The editor-ui is built here. |
| `scripts/prepare-n8n-ui.mjs` | Copies the built dist into `static/` and applies n8n's own placeholder substitutions. |
| `crates/adk-server/static/n8n-editor-ui/` | The processed, served SPA dist. |
| `crates/adk-server/src/dev_ui/n8n.rs` | Boot/auth `/rest/*` + `/types/*` stubs, SPA serving, time helpers. |
| `crates/adk-server/src/dev_ui/n8n/nodes.rs` | The `/types/nodes.json` node catalog (manual trigger, ADK Agent, HTTP Tool). |
| `crates/adk-server/src/dev_ui/n8n/workflows.rs` | File-backed workflow store + `/rest/workflows` CRUD. |
| `crates/adk-server/src/dev_ui/n8n/push.rs` | `pushRef`-keyed SSE registry (`/rest/push`). |
| `crates/adk-server/src/dev_ui/n8n/run.rs` | `/rest/workflows/:id/run` → runs the ADK Agent node, streams the push sequence. |
| `crates/adk-server/.n8n-data/` | Saved workflows (one JSON per workflow; gitignored). |

## Rebuilding the UI

```bash
# 1. Build the editor-ui SPA in the upstream clone
cd n8n
pnpm install --filter "n8n-editor-ui..."
pnpm turbo run build --filter=n8n-editor-ui
cd ..

# 2. Stage + preprocess the dist into the server's static dir
node scripts/prepare-n8n-ui.mjs

# 3. Run the dev server and open the editor
cargo run -p adk-server          # http://localhost:8091
```

`prepare-n8n-ui.mjs` mirrors what n8n's own server does in
`packages/cli/src/commands/start.ts` (`generateStaticAssets`): it replaces
`/{{BASE_PATH}}/` → `/`, injects the `n8n:config:rest-endpoint` meta tag
(base64 `rest`), and resolves `{{REST_ENDPOINT}}`. The SPA then talks
same-origin to `/rest` on adk-server.

## Backend compatibility surface (`dev_ui/n8n.rs`)

The editor boots by calling n8n's REST contract. We implement the minimum to
reach the canvas as a single local **owner** (no auth wall):

- **Boot-critical**: `GET /rest/settings` (FrontendSettings with
  `showSetupOnFirstLoad=false`, `pushBackend=sse`, telemetry/posthog off),
  `GET /rest/login` (owner user), `GET /rest/projects/{my-projects,personal,count}`,
  `GET /types/{nodes,credentials}.json` (raw arrays).
- **Push**: `GET /rest/push` is a no-op SSE stream (keeps the connection alive;
  live run events can be layered on later).
- **Stubs**: `workflows`, `credentials`, `tags`, `variables`, `users`
  (paginated `{count, items}`), `roles`, etc. return empty-but-valid bodies.
- A `/rest/{*rest}` catch-all returns `{"data": {}}` so a stray boot call never
  yields an HTML 404 the SPA's JSON parser would choke on.

Conventions: every `/rest/*` body is wrapped in a `{ "data": ... }` envelope
(the client unwraps `.data`); `/types/*.json` are bare arrays.

## Node catalog → run flow

The canvas is usable end-to-end:
- **Palette** — `/types/nodes.json` serves a manual trigger, an **ADK Agent**
  node (model / instructions / prompt / tools), and an HTTP Tool node.
- **Persistence** — workflows save/list/open/delete via `/rest/workflows*`
  (file-backed under `.n8n-data/workflows/`).
- **Run** (`run.rs`) — clicking *Execute workflow* POSTs
  `/rest/workflows/:id/run`; the run task **walks the full `connections` graph**
  in topological order and streams the per-node push sequence
  (`executionStarted`, then `nodeExecuteBefore → nodeExecuteAfter →
  nodeExecuteAfterData` for each node, then `executionFinished`) to the tab's
  `pushRef`, so the canvas animates and shows each node's output items.

### Node catalog (`nodes.rs`, 9 nodes) and executors (`run.rs`)
- **manual trigger** → emits one empty item (pin data overrides it).
- **ADK Agent** → resolves `instructions` / `text`, runs
  `OpenAiAgent::run_with_tools` with the selected tools — or `complete()` when
  none. Output: `{ output, toolCalls }`.
- **ADK Sub-Agent** → same, framed for a delegated `task`.
- **HTTP Tool** → per item, resolves `method` / `url`, injects a referenced
  `httpHeaderAuth` credential header, requests via `tools::http_request`. Output:
  `{ statusCode, body }`.
- **IF** → two outputs; routes each item by a truthy `condition` expression.
- **Edit Fields (Set)** → sets/overrides fields (values may be expressions).
- **Code** → runs JS over the input `items` and returns items (`expr::run_code`).
- **Merge** → two inputs; `append` (concatenate) or `combine` (merge fields of
  item *i* from each input).
- **ADK Memory** → store input items into / retrieve them from a named in-process
  namespace (`DevUiState.memory`), with an optional substring `query`.

Agent tool catalog (`openai.rs`): `roll_die`, `check_prime`, `httpRequest`,
`get_time`, `calculator` (the last evaluates JS math via `expr::eval_math`).

### Supported execution features
- **Graph** — topological walk; **per-input-index** gathering: fan-in
  concatenates within a slot, while a multi-input node (Merge) keeps inputs 0/1
  separate (connection `index` honoured).
- **Partial runs** — the run request body is honoured: `destinationNode`
  (run-to-node: only the destination's ancestors execute), `runData` (cached
  upstream outputs are reused), `dirtyNodeNames` (force re-execution), and
  `triggerToStartFrom` (start a trigger with supplied data).
- **Pin data** — `workflow.pinData[node]` short-circuits a node.
- **Expressions** (`expr.rs`) — embedded **JS engine (boa)** evaluates
  `={{ ... }}` with `$json`, `$node["X"].json`, `$(...)`, `$input`, `$now`,
  `$today`, `$workflow`, `$itemIndex` in scope. `$now`/`$today` are a luxon-style
  `DateTime` shim (`.plus/.minus/.startOf/.diff/.toFormat/.toISO`, `.year`…),
  zone-shifted by `ADK_TZ_OFFSET_MINUTES` (default 0 = UTC; ISO carries the
  offset, e.g. `…-04:00`).
- **Credentials** — file-backed store + CRUD at `/rest/credentials`, types at
  `/types/credentials.json`; the HTTP Tool node consumes a referenced
  `httpHeaderAuth` credential at run time.

### Icons
Generic flow nodes use n8n 2.x's Lucide names (the `fa:`→Lucide compat map in
`@n8n/design-system/.../N8nIcon/icons.ts`; unmapped names render as `?`). The
ADK integration nodes (Agent, Sub-Agent, HTTP Tool, Memory) ship branded SVGs
served from `/adk-icons/*` (`icons.rs`) via each node's `iconUrl`, which the
editor resolves with `prefixBaseUrl` and renders over the Lucide fallback.

### Recently added
- IANA timezones (`ADK_TZ`, DST-aware) alongside `ADK_TZ_OFFSET_MINUTES`.
- Merge with **N inputs** (dynamic `inputs` expression + `numberInputs`).
- Wait node: **timed** (`resume: timeInterval`) or **webhook** (HITL) resume.
- `$execution` (id / mode / resumeUrl) and a `$binary` placeholder in expressions.

## Next steps

- **Binary data** — items carry only `json`; no real `$binary` / file payloads.
- **Expression edge cases** — `$node["X"]` exposes only the first output item;
  no `$execution.customData`, `$prevNode`, or `specificTime` Wait mode.
- **Merge modes** — append / combine-by-position only (no combine-by-key,
  multiplex, or chooseBranch).
