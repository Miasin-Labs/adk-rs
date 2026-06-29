# Changelog

## Dev UI: the real n8n editor, wired to adk-rs

The `adk-server` dev UI is now the **real, verbatim upstream n8n editor** (the
Vue 3 SPA), served by the Rust server and driven by the adk-rs runner — not a
hand-written shell. See [`docs/n8n-ui.md`](docs/n8n-ui.md) for architecture and
rebuild steps.

### Serving the real SPA
- Removed the lightweight hand-written UI shell.
- Build the upstream editor-ui and stage it via `scripts/prepare-n8n-ui.mjs`,
  which applies n8n's own placeholder substitutions (`{{BASE_PATH}}`,
  `%CONFIG_TAGS%`) so the SPA boots same-origin against adk-server's `/rest`.
- `dev_ui/n8n.rs` serves the SPA at the site root with an SPA-aware fallback.

### Backend `/rest` boot contract (`dev_ui/n8n.rs`)
- Implements the minimal n8n boot surface so the SPA reaches its canvas with no
  signin/setup wall: `/rest/settings`, `/rest/login`, `/rest/projects/*`,
  `/types/{nodes,credentials}.json`, paginated `/rest/users`, plus empty-but-valid
  stubs and a `/rest/*` catch-all.
- `pushRef`-keyed SSE push registry (`dev_ui/n8n/push.rs`) drives live canvas
  updates.

### Workflow engine (`dev_ui/n8n/run.rs`)
- File-backed workflow + credential stores (`/rest/workflows`, `/rest/credentials`);
  credential types at `/types/credentials.json`.
- Topological graph execution with per-input-index gathering (fan-in) and
  multiple outputs (branching).
- **Node catalog**: ADK Agent, ADK Sub-Agent, HTTP Tool, IF, Edit Fields (Set),
  Code, Merge, ADK Memory, Wait.
- **Agent tools**: `roll_die`, `check_prime`, `httpRequest`, `get_time`,
  `calculator`.
- **Expressions** (`dev_ui/n8n/expr.rs`): embedded JS engine (boa) evaluating
  `={{ ... }}` with `$json`, `$node`, `$(...)`, `$input`, `$now`/`$today`
  (luxon-style `DateTime`), `$workflow`, `$itemIndex`.
- **Timezones**: `ADK_TZ` (IANA, DST-aware) or `ADK_TZ_OFFSET_MINUTES`.
- **Partial runs**: `destinationNode` (run-to-node), `runData` (reuse cached
  outputs), `dirtyNodeNames`, `triggerToStartFrom`.
- **Pin data**, **credentials** injected into the HTTP Tool node.
- **Merge**: 2+ inputs (dynamic `inputs` expression + `numberInputs`), append
  or combine-by-position.
- **Wait / human-in-the-loop**: a Wait node either waits a fixed time
  (`timeInterval`) or suspends (`executionWaiting`) and resumes via
  `POST /webhook-waiting/{executionId}`.
- **`$execution`** (id / mode / resumeUrl) and a `$binary` placeholder in
  expressions.
- Branded SVG icons for the ADK nodes served from `/adk-icons/*`.

### Dependencies
- Added `boa_engine` (JS expressions), `chrono` + `chrono-tz` (IANA zones),
  `futures` (SSE).
