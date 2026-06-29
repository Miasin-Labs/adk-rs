//! Manual workflow run + resume: `POST /rest/workflows/:id/run` and
//! `GET|POST /webhook-waiting/:executionId`.
//!
//! A spawned task walks the workflow graph (topological order over
//! `connections`) and streams the n8n push sequence per node. The engine
//! supports: multiple inputs (fan-in) and outputs (IF), pinned data, JS
//! expressions, partial runs (destinationNode / runData / dirtyNodeNames /
//! triggerToStartFrom), and a Wait node that suspends the run (`executionWaiting`)
//! until a resume call drives it to completion.

use std::collections::{HashMap, HashSet, VecDeque};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde_json::{Map, Value, json};

use super::super::DevUiState;
use super::expr::{self, ExprContext};

const WAIT_TYPE: &str = "adk-nodes.wait";

pub(crate) async fn run_workflow(
    Path(id): Path<String>,
    State(state): State<DevUiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
    // Parse the run payload tolerantly (any content-type). It carries optional
    // partial-run controls: triggerToStartFrom, destinationNode, runData (cached
    // outputs to reuse), and dirtyNodeNames.
    let payload: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    let push_ref = headers
        .get("push-ref")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let execution_id = state.workflows.next_execution_id();
    let workflow = state.workflows.get(&id);
    tokio::spawn(execute(
        state.clone(),
        workflow,
        id,
        execution_id.clone(),
        push_ref,
        payload,
    ));
    Json(json!({ "data": { "executionId": execution_id, "waitingForWebhook": false } }))
}

/// `GET|POST /webhook-waiting/:executionId` — resume a suspended run.
pub(crate) async fn resume_webhook(
    Path(execution_id): Path<String>,
    State(state): State<DevUiState>,
    body: Bytes,
) -> Json<Value> {
    let resume_data: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if state.workflows.peek_waiting(&execution_id).is_none() {
        return Json(json!({ "message": "No execution is waiting for this id" }));
    }
    tokio::spawn(resume(state.clone(), execution_id.clone(), resume_data));
    Json(json!({ "message": "Workflow was resumed", "executionId": execution_id }))
}

fn str_field<'a>(node: &'a Value, key: &str) -> &'a str {
    node.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn node_type(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or_default()
}

/// One output item carrying a JSON payload.
fn item(json: Value) -> Value {
    json!({ "json": json, "pairedItem": { "item": 0 } })
}

fn item_json(value: &Value) -> Value {
    value.get("json").cloned().unwrap_or_else(|| json!({}))
}

/// Output slots (`outputs[outputIndex] -> items`) extracted from an ITaskData's
/// `data.main`, used to seed cached runData / triggerToStartFrom outputs.
fn task_data_outputs(task_data: &Value) -> Option<Vec<Vec<Value>>> {
    let main = task_data.get("data")?.get("main")?.as_array()?;
    Some(
        main.iter()
            .map(|output| output.as_array().cloned().unwrap_or_default())
            .collect(),
    )
}

/// All transitive ancestors of `node` (reverse walk over the input edges).
fn ancestors(node: &str, inputs: &HashMap<String, Vec<(String, usize, usize)>>) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut queue = VecDeque::from([node.to_owned()]);
    while let Some(current) = queue.pop_front() {
        for (source, _, _) in inputs.get(&current).into_iter().flatten() {
            if set.insert(source.clone()) {
                queue.push_back(source.clone());
            }
        }
    }
    set
}

/// Immutable context shared by the initial run and any resume continuation.
struct RunCtx {
    state: DevUiState,
    push_ref: String,
    execution_id: String,
    workflow: Value,
    workflow_id: String,
    workflow_meta: Value,
    nodes: HashMap<String, Value>,
    order: Vec<String>,
    inputs: HashMap<String, Vec<(String, usize, usize)>>,
    pin_data: Value,
    consider: HashSet<String>,
    dirty: HashSet<String>,
    destination: Option<String>,
    trigger_start_name: Option<String>,
    trigger_start_outputs: Option<Vec<Vec<Value>>>,
}

impl RunCtx {
    fn send(&self, message: &Value) {
        self.state.push.send(&self.push_ref, message);
    }

    /// Gather a node's input slots (per input index) from prior outputs.
    fn input_slots(&self, name: &str, run_data: &HashMap<String, Vec<Vec<Value>>>) -> Vec<Vec<Value>> {
        let mut slots: Vec<Vec<Value>> = Vec::new();
        for (source, output_index, input_index) in self.inputs.get(name).into_iter().flatten() {
            let items = run_data
                .get(source)
                .and_then(|outputs| outputs.get(*output_index))
                .cloned()
                .unwrap_or_default();
            if slots.len() <= *input_index {
                slots.resize(*input_index + 1, Vec::new());
            }
            slots[*input_index].extend(items);
        }
        slots
    }

    fn source_field(&self, name: &str) -> Value {
        self.inputs
            .get(name)
            .and_then(|sources| sources.first())
            .map_or_else(|| json!([]), |(previous, _, _)| json!([{ "previousNode": previous }]))
    }
}

enum Outcome {
    Done,
    Waiting { node: String, next_pos: usize },
}

async fn execute(
    state: DevUiState,
    workflow: Option<Value>,
    workflow_id: String,
    execution_id: String,
    push_ref: Option<String>,
    payload: Value,
) {
    let Some(push_ref) = push_ref else { return };
    let workflow = workflow.unwrap_or_else(|| json!({}));
    let workflow_name = str_field(&workflow, "name").to_owned();
    let workflow_meta = json!({ "id": workflow_id, "name": workflow_name, "executionId": execution_id });

    let node_array = workflow
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut nodes: HashMap<String, Value> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    for node in &node_array {
        if let Some(name) = node.get("name").and_then(Value::as_str) {
            nodes.insert(name.to_owned(), node.clone());
            names.push(name.to_owned());
        }
    }
    let (edges, inputs) = build_adjacency(workflow.get("connections"));
    let pin_data = workflow.get("pinData").cloned().unwrap_or_else(|| json!({}));

    // Partial-run controls from the request payload.
    let destination = payload.get("destinationNode").and_then(|node| {
        node.as_str()
            .map(str::to_owned)
            .or_else(|| node.get("nodeName").and_then(Value::as_str).map(str::to_owned))
    });
    let dirty: HashSet<String> = payload
        .get("dirtyNodeNames")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    let trigger_start = payload.get("triggerToStartFrom");
    let trigger_start_name = trigger_start
        .and_then(|trigger| trigger.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let trigger_start_outputs = trigger_start
        .and_then(|trigger| trigger.get("data"))
        .and_then(task_data_outputs);

    let consider: HashSet<String> = match &destination {
        Some(node) => {
            let mut set = ancestors(node, &inputs);
            set.insert(node.clone());
            set
        }
        None => names.iter().cloned().collect(),
    };

    // Seed cached outputs from the payload's prior runData.
    let mut run_data: HashMap<String, Vec<Vec<Value>>> = HashMap::new();
    if let Some(prior) = payload.get("runData").and_then(Value::as_object) {
        for (name, runs) in prior {
            if let Some(outputs) = runs
                .as_array()
                .and_then(|runs| runs.first())
                .and_then(task_data_outputs)
            {
                run_data.insert(name.clone(), outputs);
            }
        }
    }

    let ctx = RunCtx {
        state,
        push_ref,
        execution_id,
        workflow,
        workflow_id,
        workflow_meta,
        nodes,
        order: topo_order(&names, &edges),
        inputs,
        pin_data,
        consider,
        dirty,
        destination,
        trigger_start_name,
        trigger_start_outputs,
    };

    ctx.send(&json!({ "type": "executionStarted", "data": {
        "executionId": ctx.execution_id,
        "workflowId": ctx.workflow_id,
        "workflowName": ctx.workflow_meta["name"],
        "mode": "manual",
        "source": "user-manual",
        "retryOf": null,
        "startedAt": super::iso_now(),
        "flattedRunData": "{}",
    }}));

    let mut persisted = Map::new();
    let mut index = 0;
    let mut status = "success";
    let outcome = drive(&ctx, &mut run_data, &mut persisted, &mut index, &mut status, 0).await;
    finish(&ctx, run_data, persisted, index, status, outcome).await;
}

/// Process the topo order from `start_pos`, returning whether the run completed
/// or hit a Wait node.
async fn drive(
    ctx: &RunCtx,
    run_data: &mut HashMap<String, Vec<Vec<Value>>>,
    persisted: &mut Map<String, Value>,
    index: &mut usize,
    overall_status: &mut &'static str,
    start_pos: usize,
) -> Outcome {
    for pos in start_pos..ctx.order.len() {
        let name = ctx.order[pos].clone();
        if !ctx.consider.contains(&name) {
            continue;
        }
        let Some(node) = ctx.nodes.get(&name).cloned() else {
            continue;
        };
        let is_cached = run_data.contains_key(&name)
            && !ctx.dirty.contains(&name)
            && ctx.destination.as_deref() != Some(name.as_str());
        if is_cached {
            continue;
        }

        let input_slots = ctx.input_slots(&name, run_data);
        let source_field = ctx.source_field(&name);
        let started = super::now_ms();
        ctx.send(&json!({ "type": "nodeExecuteBefore", "data": {
            "executionId": ctx.execution_id, "nodeName": name,
            "data": { "startTime": started, "executionIndex": *index, "source": source_field, "hints": [] }
        }}));

        // Wait node: pause for a fixed time, or suspend until a resume call.
        if node_type(&node) == WAIT_TYPE {
            ctx.send(&json!({ "type": "executionWaiting", "data": {
                "executionId": ctx.execution_id, "source": "user-manual"
            }}));
            let params = node.get("parameters").cloned().unwrap_or_else(|| json!({}));
            if params.get("resume").and_then(Value::as_str).unwrap_or("timeInterval") == "webhook" {
                return Outcome::Waiting { node: name.clone(), next_pos: pos + 1 };
            }
            // Timed wait: sleep inline (capped at 1h for a dev server), then pass
            // the input through and continue.
            let amount = params.get("amount").and_then(Value::as_f64).unwrap_or(0.0).max(0.0);
            let unit_seconds = match params.get("unit").and_then(Value::as_str).unwrap_or("seconds") {
                "minutes" => 60.0,
                "hours" => 3600.0,
                _ => 1.0,
            };
            let seconds = (amount * unit_seconds).min(3600.0);
            tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
            let outputs = vec![input_slots.first().cloned().unwrap_or_else(|| vec![item(json!({}))])];
            run_data.insert(name.clone(), outputs.clone());
            emit_after(ctx, &name, started, *index, &source_field, "success", &outputs, persisted);
            *index += 1;
            continue;
        }

        let nodes_json = node_output_map(run_data);
        let (status, outputs) = if let Some(pinned) = ctx
            .pin_data
            .get(&name)
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
        {
            ("success", vec![pinned.clone()])
        } else if ctx.trigger_start_name.as_deref() == Some(name.as_str())
            && ctx.trigger_start_outputs.is_some()
        {
            ("success", ctx.trigger_start_outputs.clone().unwrap_or_default())
        } else {
            execute_node(&ctx.state, &node, &input_slots, &nodes_json, &ctx.workflow_meta).await
        };
        if status == "error" {
            *overall_status = "error";
        }
        run_data.insert(name.clone(), outputs.clone());
        emit_after(ctx, &name, started, *index, &source_field, status, &outputs, persisted);
        *index += 1;
        if status == "error" {
            break;
        }
    }
    Outcome::Done
}

/// Emit `nodeExecuteAfter`/`nodeExecuteAfterData` for a finished node and record
/// its task data for persistence.
fn emit_after(
    ctx: &RunCtx,
    name: &str,
    started: u64,
    index: usize,
    source_field: &Value,
    status: &str,
    outputs: &[Vec<Value>],
    persisted: &mut Map<String, Value>,
) {
    let error = outputs
        .first()
        .and_then(|items| items.first())
        .map(item_json)
        .and_then(|json| json.get("error").cloned());
    let counts: Vec<usize> = outputs.iter().map(Vec::len).collect();
    let task_data = json!({
        "startTime": started,
        "executionIndex": index,
        "source": source_field,
        "executionTime": super::now_ms().saturating_sub(started),
        "executionStatus": status,
        "error": if status == "error" { error.unwrap_or(json!(null)) } else { json!(null) },
        "data": { "main": outputs }
    });
    let mut trimmed = task_data.clone();
    if let Some(object) = trimmed.as_object_mut() {
        object.remove("data");
    }
    ctx.send(&json!({ "type": "nodeExecuteAfter", "data": {
        "executionId": ctx.execution_id, "nodeName": name, "data": trimmed,
        "itemCountByConnectionType": { "main": counts }
    }}));
    ctx.send(&json!({ "type": "nodeExecuteAfterData", "data": {
        "executionId": ctx.execution_id, "nodeName": name, "data": task_data.clone(),
        "itemCountByConnectionType": { "main": counts }
    }}));
    persisted.insert(name.to_owned(), json!([task_data]));
}

/// Persist + finalize the run, or save suspended state on a Wait.
async fn finish(
    ctx: &RunCtx,
    run_data: HashMap<String, Vec<Vec<Value>>>,
    persisted: Map<String, Value>,
    index: usize,
    status: &str,
    outcome: Outcome,
) {
    match outcome {
        Outcome::Waiting { node, next_pos } => {
            ctx.state.workflows.save_waiting(
                &ctx.execution_id,
                json!({
                    "workflow": ctx.workflow,
                    "workflowId": ctx.workflow_id,
                    "runData": serde_json::to_value(&run_data).unwrap_or(json!({})),
                    "persisted": Value::Object(persisted),
                    "order": ctx.order,
                    "nextPos": next_pos,
                    "index": index,
                    "pushRef": ctx.push_ref,
                    "overallStatus": status,
                    "waitNode": node,
                }),
            );
            ctx.state.workflows.record_execution(
                ctx.execution_id.clone(),
                json!({ "id": ctx.execution_id, "finished": false, "mode": "manual", "status": "waiting" }),
            );
        }
        Outcome::Done => {
            ctx.state.workflows.record_execution(
                ctx.execution_id.clone(),
                json!({
                    "id": ctx.execution_id, "finished": true, "mode": "manual", "status": status,
                    "workflowData": ctx.workflow,
                    "data": { "resultData": { "runData": Value::Object(persisted) } }
                }),
            );
            ctx.send(&json!({ "type": "executionFinished", "data": {
                "executionId": ctx.execution_id, "workflowId": ctx.workflow_id,
                "status": status, "source": "user-manual"
            }}));
        }
    }
}

/// Resume a suspended run: replay the Wait node's output, then drive the rest.
async fn resume(state: DevUiState, execution_id: String, resume_data: Value) {
    let Some(saved) = state.workflows.take_waiting(&execution_id) else {
        return;
    };
    let workflow = saved.get("workflow").cloned().unwrap_or_else(|| json!({}));
    let workflow_id = saved.get("workflowId").and_then(Value::as_str).unwrap_or("").to_owned();
    let push_ref = saved.get("pushRef").and_then(Value::as_str).unwrap_or("default").to_owned();
    let wait_node = saved.get("waitNode").and_then(Value::as_str).unwrap_or("").to_owned();
    let next_pos = saved.get("nextPos").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut index = saved.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut status = if saved.get("overallStatus").and_then(Value::as_str) == Some("error") {
        "error"
    } else {
        "success"
    };
    let mut run_data: HashMap<String, Vec<Vec<Value>>> = saved
        .get("runData")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let mut persisted = saved
        .get("persisted")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let order: Vec<String> = saved
        .get("order")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();

    let workflow_name = str_field(&workflow, "name").to_owned();
    let workflow_meta = json!({ "id": workflow_id, "name": workflow_name, "executionId": execution_id });
    let node_array = workflow.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut nodes: HashMap<String, Value> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    for node in &node_array {
        if let Some(name) = node.get("name").and_then(Value::as_str) {
            nodes.insert(name.to_owned(), node.clone());
            names.push(name.to_owned());
        }
    }
    let (_edges, inputs) = build_adjacency(workflow.get("connections"));
    let pin_data = workflow.get("pinData").cloned().unwrap_or_else(|| json!({}));

    let ctx = RunCtx {
        state,
        push_ref,
        execution_id,
        workflow,
        workflow_id,
        workflow_meta,
        nodes,
        order,
        inputs,
        pin_data,
        consider: names.into_iter().collect(),
        dirty: HashSet::new(),
        destination: None,
        trigger_start_name: None,
        trigger_start_outputs: None,
    };

    // The Wait node's output: the resume payload (wrapped), else pass through its
    // input. Emit its nodeExecuteAfter so the canvas shows it completed.
    let wait_output = match &resume_data {
        Value::Null => ctx
            .input_slots(&wait_node, &run_data)
            .into_iter()
            .next()
            .map(|items| vec![items])
            .unwrap_or_else(|| vec![vec![item(json!({}))]]),
        other => vec![vec![item(json!({ "resumed": true, "data": other }))]],
    };
    let started = super::now_ms();
    run_data.insert(wait_node.clone(), wait_output.clone());
    emit_after(&ctx, &wait_node, started, index, &ctx.source_field(&wait_node), "success", &wait_output, &mut persisted);
    index += 1;

    let outcome = drive(&ctx, &mut run_data, &mut persisted, &mut index, &mut status, next_pos).await;
    finish(&ctx, run_data, persisted, index, status, outcome).await;
}

/// Build a `{ nodeName: firstOutputItemJson }` map for expression context.
fn node_output_map(run_data: &HashMap<String, Vec<Vec<Value>>>) -> Value {
    let mut map = Map::new();
    for (name, outputs) in run_data {
        let json = outputs
            .first()
            .and_then(|items| items.first())
            .map(item_json)
            .unwrap_or_else(|| json!({}));
        map.insert(name.clone(), json);
    }
    Value::Object(map)
}

/// Timezone configuration: a named IANA zone (DST-aware) or a fixed offset.
enum TzConfig {
    Zone(chrono_tz::Tz),
    Fixed(i64),
}

/// Resolved once: `ADK_TZ` (IANA name) takes precedence, else
/// `ADK_TZ_OFFSET_MINUTES` (fixed), else UTC.
fn tz_config() -> &'static TzConfig {
    use std::sync::OnceLock;
    static CFG: OnceLock<TzConfig> = OnceLock::new();
    CFG.get_or_init(|| {
        if let Ok(name) = std::env::var("ADK_TZ")
            && let Ok(zone) = name.parse::<chrono_tz::Tz>()
        {
            return TzConfig::Zone(zone);
        }
        let offset = std::env::var("ADK_TZ_OFFSET_MINUTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        TzConfig::Fixed(offset)
    })
}

/// Timezone offset (minutes) applied to `$now`/`$today`. For a named zone this is
/// computed for the current instant, so DST is honoured.
fn tz_offset_minutes() -> i64 {
    match tz_config() {
        TzConfig::Zone(zone) => {
            use chrono::Offset;
            i64::from(chrono::Utc::now().with_timezone(zone).offset().fix().local_minus_utc()) / 60
        }
        TzConfig::Fixed(offset) => *offset,
    }
}

fn context_for(item_json: Value, nodes: &Value, workflow: &Value, item_index: usize) -> ExprContext {
    ExprContext {
        json: item_json,
        nodes: nodes.clone(),
        now_ms: super::now_ms(),
        workflow: workflow.clone(),
        item_index,
        tz_offset_minutes: tz_offset_minutes(),
    }
}

/// Run a single node, returning `(status, outputs[outputIndex][items])`.
async fn execute_node(
    state: &DevUiState,
    node: &Value,
    input_slots: &[Vec<Value>],
    nodes: &Value,
    workflow: &Value,
) -> (&'static str, Vec<Vec<Value>>) {
    let node_type = str_field(node, "type");
    let parameters = node.get("parameters").cloned().unwrap_or_else(|| json!({}));
    let input_items: Vec<Value> = input_slots.first().cloned().unwrap_or_default();
    let working_items: Vec<Value> = if input_items.is_empty() {
        vec![item(json!({}))]
    } else {
        input_items.clone()
    };

    if node_type.ends_with("manualTrigger") || is_trigger(node) {
        return ("success", vec![vec![item(json!({}))]]);
    }

    match node_type {
        super::nodes::ADK_AGENT_TYPE => {
            run_agent(state, &parameters, "You are a helpful assistant.", "text", &input_items, nodes, workflow).await
        }
        "adk-nodes.subAgent" => {
            run_agent(
                state,
                &parameters,
                "You are a focused sub-agent. Complete the delegated task precisely and concisely.",
                "task",
                &input_items,
                nodes,
                workflow,
            )
            .await
        }
        "adk-nodes.httpTool" => {
            let mut out = Vec::new();
            let mut status = "success";
            for (index, current) in working_items.iter().enumerate() {
                let context = context_for(item_json(current), nodes, workflow, index);
                let method = expr::resolve(parameters.get("method"), "GET", &context);
                let url = expr::resolve(parameters.get("url"), "", &context);
                let headers = credential_headers(state, node);
                let response = super::super::tools::http_request(&method, &url, None, &headers).await;
                if response.get("error").is_some() {
                    status = "error";
                }
                out.push(item(response));
            }
            (status, vec![out])
        }
        "adk-nodes.if" => {
            let (mut truthy, mut falsy) = (Vec::new(), Vec::new());
            for (index, current) in working_items.iter().enumerate() {
                let context = context_for(item_json(current), nodes, workflow, index);
                let resolved = expr::resolve(parameters.get("condition"), "false", &context);
                if is_truthy(&resolved) {
                    truthy.push(current.clone());
                } else {
                    falsy.push(current.clone());
                }
            }
            ("success", vec![truthy, falsy])
        }
        "adk-nodes.set" => {
            // Accept both a plain `[ {name,value} ]` array and the editor's
            // fixedCollection shape `{ field: [ {name,value} ] }`.
            let fields_param = parameters.get("fields");
            let fields = fields_param
                .and_then(Value::as_array)
                .cloned()
                .or_else(|| fields_param.and_then(|fields| fields.get("field")).and_then(Value::as_array).cloned())
                .unwrap_or_default();
            let mut out = Vec::new();
            for (index, current) in working_items.iter().enumerate() {
                let mut json = item_json(current);
                let context = context_for(json.clone(), nodes, workflow, index);
                for field in &fields {
                    let key = str_field(field, "name");
                    if key.is_empty() {
                        continue;
                    }
                    let value = expr::resolve(field.get("value"), "", &context);
                    if let Some(object) = json.as_object_mut() {
                        object.insert(key.to_owned(), coerce(&value));
                    }
                }
                out.push(item(json));
            }
            ("success", vec![out])
        }
        "adk-nodes.code" => {
            let code = str_field(node.get("parameters").unwrap_or(&Value::Null), "code");
            let first = working_items.first().map(item_json).unwrap_or_else(|| json!({}));
            let context = context_for(first, nodes, workflow, 0);
            match expr::run_code(code, &input_items, &context) {
                Ok(items) => ("success", vec![items]),
                Err(error) => ("error", vec![vec![item(json!({ "error": error }))]]),
            }
        }
        "adk-nodes.merge" => {
            let mode = parameters.get("mode").and_then(Value::as_str).unwrap_or("append");
            let merged: Vec<Value> = if mode == "combine" {
                // Merge the fields of item `index` across every input.
                let count = input_slots.iter().map(Vec::len).max().unwrap_or(0);
                (0..count)
                    .map(|index| {
                        let mut json = json!({});
                        for slot in input_slots {
                            if let Some(extra) = slot.get(index).map(item_json)
                                && let (Some(target), Some(source)) =
                                    (json.as_object_mut(), extra.as_object())
                            {
                                for (key, value) in source {
                                    target.insert(key.clone(), value.clone());
                                }
                            }
                        }
                        item(json)
                    })
                    .collect()
            } else {
                // Append: every input's items, in order.
                input_slots.iter().flatten().cloned().collect()
            };
            ("success", vec![merged])
        }
        "adk-nodes.memory" => {
            let first = working_items.first().map(item_json).unwrap_or_else(|| json!({}));
            let context = context_for(first, nodes, workflow, 0);
            let namespace = expr::resolve(parameters.get("namespace"), "default", &context);
            let operation = parameters.get("operation").and_then(Value::as_str).unwrap_or("store");
            if operation == "retrieve" {
                let query = expr::resolve(parameters.get("query"), "", &context).to_lowercase();
                let stored = state.memory_all(&namespace);
                let items = stored
                    .into_iter()
                    .filter(|json| query.is_empty() || json.to_string().to_lowercase().contains(&query))
                    .map(item)
                    .collect();
                ("success", vec![items])
            } else {
                let stored: Vec<Value> = working_items.iter().map(item_json).collect();
                state.memory_append(&namespace, &stored);
                ("success", vec![working_items.clone()])
            }
        }
        // Wait is handled by `drive`; unknown node types pass their input through.
        _ => ("success", vec![input_items.clone()]),
    }
}

/// Shared agent/sub-agent executor: resolves instructions + prompt with
/// expressions and runs the model with the node's selected tools.
async fn run_agent(
    state: &DevUiState,
    parameters: &Value,
    default_instructions: &str,
    prompt_key: &str,
    input_items: &[Value],
    nodes: &Value,
    workflow: &Value,
) -> (&'static str, Vec<Vec<Value>>) {
    let first = input_items.first().map(item_json).unwrap_or_else(|| json!({}));
    let context = context_for(first, nodes, workflow, 0);
    let instructions = expr::resolve(parameters.get("instructions"), default_instructions, &context);
    let prompt = expr::resolve(parameters.get(prompt_key), "", &context);
    let tools: Vec<String> = parameters
        .get("tools")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    let Some(model) = state.agent() else {
        return (
            "error",
            vec![vec![item(json!({ "error": "No model configured. Set OPENAI_API_KEY (and optionally ADK_OPENAI_MODEL)." }))]],
        );
    };
    let result = if tools.is_empty() {
        model.complete(&instructions, &prompt).await.map(|text| (text, Vec::new()))
    } else {
        model.run_with_tools(&instructions, &prompt, &tools).await.map(|run| {
            let calls = run
                .tools
                .iter()
                .map(|observation| json!({ "name": observation.name, "args": observation.args, "response": observation.response }))
                .collect::<Vec<_>>();
            (run.text, calls)
        })
    };
    match result {
        Ok((text, tool_calls)) => (
            "success",
            vec![vec![item(json!({ "output": text, "toolCalls": tool_calls }))]],
        ),
        Err(error) => ("error", vec![vec![item(json!({ "error": error }))]]),
    }
}

/// JS-ish truthiness for an IF condition's resolved string value.
fn is_truthy(value: &str) -> bool {
    !matches!(value.trim(), "" | "false" | "0" | "null" | "undefined" | "NaN")
}

/// Coerce a resolved string into a JSON value (number/bool/null fall through to
/// their typed form; everything else stays a string).
fn coerce(value: &str) -> Value {
    match value {
        "true" => json!(true),
        "false" => json!(false),
        "null" => json!(null),
        other => other
            .parse::<i64>()
            .map(|number| json!(number))
            .or_else(|_| other.parse::<f64>().map(|number| json!(number)))
            .unwrap_or_else(|_| json!(other)),
    }
}

fn is_trigger(node: &Value) -> bool {
    node.get("type")
        .and_then(Value::as_str)
        .is_some_and(|node_type| node_type.to_ascii_lowercase().contains("trigger"))
}

/// Resolve a stored `httpHeaderAuth` credential referenced by the node into
/// request headers.
fn credential_headers(state: &DevUiState, node: &Value) -> Vec<(String, String)> {
    let Some(reference) = node
        .get("credentials")
        .and_then(|credentials| credentials.get("httpHeaderAuth"))
        .and_then(|credential| credential.get("id"))
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };
    let Some(data) = state.credentials.data(reference) else {
        return Vec::new();
    };
    let name = data.get("name").and_then(Value::as_str).unwrap_or("");
    let value = data.get("value").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() {
        Vec::new()
    } else {
        vec![(name.to_owned(), value.to_owned())]
    }
}

/// Build `(source -> targets, target -> [(source, sourceOutputIndex,
/// targetInputIndex)])` from `connections`.
fn build_adjacency(
    connections: Option<&Value>,
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<(String, usize, usize)>>) {
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    let mut inputs: HashMap<String, Vec<(String, usize, usize)>> = HashMap::new();
    let Some(object) = connections.and_then(Value::as_object) else {
        return (edges, inputs);
    };
    for (source, outputs) in object {
        let Some(main) = outputs.get("main").and_then(Value::as_array) else {
            continue;
        };
        for (output_index, output) in main.iter().enumerate() {
            for target in output.as_array().into_iter().flatten() {
                if let Some(name) = target.get("node").and_then(Value::as_str) {
                    let input_index = target.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    edges.entry(source.clone()).or_default().push(name.to_owned());
                    inputs.entry(name.to_owned()).or_default().push((source.clone(), output_index, input_index));
                }
            }
        }
    }
    (edges, inputs)
}

/// Kahn's topological sort; any leftover (cyclic) nodes are appended in order.
fn topo_order(names: &[String], edges: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut in_degree: HashMap<&str, usize> = names.iter().map(|name| (name.as_str(), 0)).collect();
    for targets in edges.values() {
        for target in targets {
            if let Some(degree) = in_degree.get_mut(target.as_str()) {
                *degree += 1;
            }
        }
    }
    let mut queue: VecDeque<String> = names
        .iter()
        .filter(|name| in_degree.get(name.as_str()) == Some(&0))
        .cloned()
        .collect();
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    while let Some(name) = queue.pop_front() {
        if !visited.insert(name.clone()) {
            continue;
        }
        order.push(name.clone());
        for target in edges.get(&name).into_iter().flatten() {
            if let Some(degree) = in_degree.get_mut(target.as_str()) {
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    queue.push_back(target.clone());
                }
            }
        }
    }
    for name in names {
        if !visited.contains(name) {
            order.push(name.clone());
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(json_value: Value) -> ExprContext {
        ExprContext {
            json: json_value,
            nodes: json!({}),
            now_ms: 1_700_000_000_000,
            workflow: json!({ "id": "w", "name": "W" }),
            item_index: 0,
            tz_offset_minutes: 0,
        }
    }

    #[test]
    fn topo_order_is_dependency_respecting() {
        let names = vec!["c".to_owned(), "a".to_owned(), "b".to_owned()];
        let mut edges = HashMap::new();
        edges.insert("a".to_owned(), vec!["b".to_owned()]);
        edges.insert("b".to_owned(), vec!["c".to_owned()]);
        let order = topo_order(&names, &edges);
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn ancestors_walks_inputs() {
        let mut inputs = HashMap::new();
        inputs.insert("c".to_owned(), vec![("b".to_owned(), 0, 0)]);
        inputs.insert("b".to_owned(), vec![("a".to_owned(), 0, 0)]);
        let set = ancestors("c", &inputs);
        assert!(set.contains("a") && set.contains("b") && !set.contains("c"));
    }

    #[test]
    fn expression_evaluates_js() {
        assert_eq!(expr::resolve(Some(&json!("={{ $json.n * 2 }}")), "", &ctx(json!({ "n": 21 }))), "42");
        assert_eq!(expr::resolve(Some(&json!("={{ $json.s.toUpperCase() }}")), "", &ctx(json!({ "s": "hi" }))), "HI");
        assert_eq!(expr::resolve(Some(&json!("literal")), "", &ctx(json!({}))), "literal");
    }

    #[test]
    fn eval_math_handles_arithmetic() {
        assert_eq!(expr::eval_math("17 * 23").unwrap() as i64, 391);
    }

    #[test]
    fn truthiness_and_coercion() {
        assert!(is_truthy("true") && is_truthy("hello") && is_truthy("5"));
        assert!(!is_truthy("false") && !is_truthy("0") && !is_truthy(""));
        assert_eq!(coerce("42"), json!(42));
        assert_eq!(coerce("true"), json!(true));
        assert_eq!(coerce("hi"), json!("hi"));
    }

    #[test]
    fn adjacency_tracks_input_index() {
        let connections = json!({
            "A": { "main": [[{ "node": "M", "type": "main", "index": 0 }]] },
            "B": { "main": [[{ "node": "M", "type": "main", "index": 1 }]] }
        });
        let (_edges, inputs) = build_adjacency(Some(&connections));
        let to_m = inputs.get("M").unwrap();
        assert!(to_m.contains(&("A".to_owned(), 0, 0)));
        assert!(to_m.contains(&("B".to_owned(), 0, 1)));
    }
}
