//! Curated `INodeTypeDescription` catalog served at `/types/nodes.json`.
//!
//! These map adk-rs concepts onto n8n nodes: a manual trigger, an **ADK Agent**
//! node (run by `run.rs` via the dev_ui `OpenAiAgent`), and a standalone HTTP
//! tool. `fa:` icons render from a bundled font, so there are no `/icons/*`
//! fetches to 404. A node's `type` in a saved workflow must equal the catalog
//! `name` exactly.

use axum::Json;
use serde_json::{Value, json};

/// `GET /types/nodes.json` — a bare array (no `{data}` envelope; `/types/*` are
/// fetched raw by the client).
pub(crate) async fn catalog() -> Json<Value> {
    Json(nodes())
}

/// `POST /rest/node-types` — lazy full-description fetch. We already ship full
/// descriptions in the catalog, so echo the whole set back, enveloped.
pub(crate) async fn node_types() -> Json<Value> {
    Json(json!({ "data": nodes() }))
}

/// The name the run handler keys the agent node off of.
pub(crate) const ADK_AGENT_TYPE: &str = "adk-nodes.adkAgent";

fn nodes() -> Value {
    json!([
        {
            "displayName": "When clicking 'Test workflow'",
            "name": "n8n-nodes-base.manualTrigger",
            "group": ["trigger"],
            "version": 1,
            "description": "Runs the flow when you click 'Test workflow' on the canvas",
            "eventTriggerDescription": "",
            "maxNodes": 1,
            "defaults": { "name": "When clicking 'Test workflow'", "color": "#909298" },
            "inputs": [],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "This node fires when you click 'Test workflow'.",
                    "name": "notice",
                    "type": "notice",
                    "default": ""
                }
            ],
            "icon": "fa:mouse-pointer",
            "iconColor": "gray",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Triggers"] }
            }
        },
        {
            "displayName": "ADK Agent",
            "name": ADK_AGENT_TYPE,
            "group": ["transform"],
            "version": 1,
            "description": "Runs an adk-rs LLM agent with tools and returns its output",
            "defaults": { "name": "ADK Agent", "color": "#1f9c8a" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Model",
                    "name": "model",
                    "type": "options",
                    "default": "gpt-4o-mini",
                    "options": [
                        { "name": "GPT-4o", "value": "gpt-4o" },
                        { "name": "GPT-4o mini", "value": "gpt-4o-mini" },
                        { "name": "GPT-3.5 Turbo", "value": "gpt-3.5-turbo" }
                    ],
                    "description": "Language model used for this agent run (set via OPENAI/ADK env on the server)"
                },
                {
                    "displayName": "Instructions (System Prompt)",
                    "name": "instructions",
                    "type": "string",
                    "typeOptions": { "rows": 4 },
                    "default": "You are a helpful assistant.",
                    "description": "System instructions that define the agent's behaviour"
                },
                {
                    "displayName": "Prompt",
                    "name": "text",
                    "type": "string",
                    "typeOptions": { "rows": 3 },
                    "default": "",
                    "description": "The user prompt sent to the agent"
                },
                {
                    "displayName": "Tools",
                    "name": "tools",
                    "type": "multiOptions",
                    "default": [],
                    "options": [
                        { "name": "HTTP Request", "value": "httpRequest" },
                        { "name": "Roll Die", "value": "roll_die" },
                        { "name": "Check Prime", "value": "check_prime" },
                        { "name": "Get Time", "value": "get_time" },
                        { "name": "Calculator", "value": "calculator" }
                    ],
                    "description": "Built-in adk-rs tools the agent may call"
                }
            ],
            "icon": "fa:robot",
            "iconUrl": "adk-icons/agent.svg",
            "iconColor": "green",
            "codex": {
                "categories": ["AI"],
                "subcategories": { "AI": ["Agents", "Root Nodes"] },
                "alias": ["LLM", "agent", "adk", "chat"]
            }
        },
        {
            "displayName": "HTTP Tool",
            "name": "adk-nodes.httpTool",
            "group": ["transform"],
            "version": 1,
            "description": "Standalone HTTP request node (also usable as an agent tool)",
            "defaults": { "name": "HTTP Tool", "color": "#2233dd" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Method",
                    "name": "method",
                    "type": "options",
                    "default": "GET",
                    "options": [
                        { "name": "GET", "value": "GET" },
                        { "name": "POST", "value": "POST" },
                        { "name": "PUT", "value": "PUT" },
                        { "name": "DELETE", "value": "DELETE" }
                    ]
                },
                {
                    "displayName": "URL",
                    "name": "url",
                    "type": "string",
                    "default": "",
                    "placeholder": "https://api.example.com/resource",
                    "required": true
                }
            ],
            "icon": "fa:globe",
            "iconUrl": "adk-icons/http.svg",
            "iconColor": "blue",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Helpers"] }
            }
        },
        {
            "displayName": "ADK Sub-Agent",
            "name": "adk-nodes.subAgent",
            "group": ["transform"],
            "version": 1,
            "description": "Delegate a focused task to a child adk-rs agent",
            "defaults": { "name": "ADK Sub-Agent", "color": "#6b4fbb" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Task",
                    "name": "task",
                    "type": "string",
                    "typeOptions": { "rows": 3 },
                    "default": "",
                    "description": "The task/goal delegated to the sub-agent (supports expressions)"
                },
                {
                    "displayName": "Tools",
                    "name": "tools",
                    "type": "multiOptions",
                    "default": [],
                    "options": [
                        { "name": "HTTP Request", "value": "httpRequest" },
                        { "name": "Roll Die", "value": "roll_die" },
                        { "name": "Check Prime", "value": "check_prime" },
                        { "name": "Get Time", "value": "get_time" },
                        { "name": "Calculator", "value": "calculator" }
                    ]
                }
            ],
            "icon": "fa:network",
            "iconUrl": "adk-icons/subagent.svg",
            "iconColor": "purple",
            "codex": {
                "categories": ["AI"],
                "subcategories": { "AI": ["Agents"] },
                "alias": ["delegate", "subagent"]
            }
        },
        {
            "displayName": "IF",
            "name": "adk-nodes.if",
            "group": ["transform"],
            "version": 1,
            "description": "Route items to the true or false branch based on a condition",
            "defaults": { "name": "IF", "color": "#408000" },
            "inputs": ["main"],
            "outputs": ["main", "main"],
            "outputNames": ["true", "false"],
            "properties": [
                {
                    "displayName": "Condition",
                    "name": "condition",
                    "type": "string",
                    "default": "={{ $json.value }}",
                    "description": "An expression; truthy items go to the 'true' output, the rest to 'false'"
                }
            ],
            "icon": "fa:code-branch",
            "iconColor": "green",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Flow"] }
            }
        },
        {
            "displayName": "Edit Fields (Set)",
            "name": "adk-nodes.set",
            "group": ["transform"],
            "version": 1,
            "description": "Set or override fields on each item",
            "defaults": { "name": "Edit Fields", "color": "#0000ff" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Fields to Set",
                    "name": "fields",
                    "type": "fixedCollection",
                    "typeOptions": { "multipleValues": true },
                    "default": {},
                    "options": [
                        {
                            "name": "field",
                            "displayName": "Field",
                            "values": [
                                { "displayName": "Name", "name": "name", "type": "string", "default": "" },
                                { "displayName": "Value", "name": "value", "type": "string", "default": "" }
                            ]
                        }
                    ]
                }
            ],
            "icon": "fa:pen",
            "iconColor": "blue",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Data Transformation"] }
            }
        },
        {
            "displayName": "Code",
            "name": "adk-nodes.code",
            "group": ["transform"],
            "version": 1,
            "description": "Run JavaScript over the input items and return items",
            "defaults": { "name": "Code", "color": "#ff6d00" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "JavaScript",
                    "name": "code",
                    "type": "string",
                    "typeOptions": { "rows": 8 },
                    "default": "// `items` are the input items; $json is the first item.\nreturn items;",
                    "description": "Use items, $json, $input. Must return an array of items."
                }
            ],
            "icon": "fa:code",
            "iconColor": "crimson",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Development"] }
            }
        },
        {
            "displayName": "Merge",
            "name": "adk-nodes.merge",
            "group": ["transform"],
            "version": 1,
            "description": "Combine items from two or more inputs",
            "defaults": { "name": "Merge", "color": "#00bbcc" },
            "inputs": "={{ ((p) => Array.from({ length: (p.numberInputs || 2) }, () => 'main'))($parameter) }}",
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Number of Inputs",
                    "name": "numberInputs",
                    "type": "options",
                    "default": 2,
                    "options": [
                        { "name": "2", "value": 2 },
                        { "name": "3", "value": 3 },
                        { "name": "4", "value": 4 },
                        { "name": "5", "value": 5 }
                    ],
                    "description": "How many inputs this node accepts"
                },
                {
                    "displayName": "Mode",
                    "name": "mode",
                    "type": "options",
                    "default": "append",
                    "options": [
                        { "name": "Append", "value": "append", "description": "Output the items of every input, in order" },
                        { "name": "Combine by Position", "value": "combine", "description": "Merge the fields of item i across all inputs" }
                    ]
                }
            ],
            "icon": "fa:layers",
            "iconColor": "blue",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Flow"] }
            }
        },
        {
            "displayName": "ADK Memory",
            "name": "adk-nodes.memory",
            "group": ["transform"],
            "version": 1,
            "description": "Store items into, or retrieve items from, a named adk-rs memory namespace",
            "defaults": { "name": "ADK Memory", "color": "#d97706" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Operation",
                    "name": "operation",
                    "type": "options",
                    "default": "store",
                    "options": [
                        { "name": "Store", "value": "store", "description": "Append the input items to the namespace" },
                        { "name": "Retrieve", "value": "retrieve", "description": "Output the items stored in the namespace" }
                    ]
                },
                {
                    "displayName": "Namespace",
                    "name": "namespace",
                    "type": "string",
                    "default": "default",
                    "description": "Memory namespace key (supports expressions)"
                },
                {
                    "displayName": "Query",
                    "name": "query",
                    "type": "string",
                    "default": "",
                    "displayOptions": { "show": { "operation": ["retrieve"] } },
                    "description": "Optional substring filter applied to stored items"
                }
            ],
            "icon": "fa:database",
            "iconUrl": "adk-icons/memory.svg",
            "iconColor": "orange",
            "codex": {
                "categories": ["AI"],
                "subcategories": { "AI": ["Memory"] }
            }
        },
        {
            "displayName": "Wait",
            "name": "adk-nodes.wait",
            "group": ["organization"],
            "version": 1,
            "description": "Pause the workflow for a set time or until it is resumed",
            "defaults": { "name": "Wait", "color": "#804050" },
            "inputs": ["main"],
            "outputs": ["main"],
            "properties": [
                {
                    "displayName": "Resume",
                    "name": "resume",
                    "type": "options",
                    "default": "timeInterval",
                    "options": [
                        { "name": "After Time Interval", "value": "timeInterval", "description": "Wait a fixed amount of time, then continue" },
                        { "name": "On Webhook Call", "value": "webhook", "description": "Suspend until POST /webhook-waiting/{executionId}" }
                    ]
                },
                {
                    "displayName": "Wait Amount",
                    "name": "amount",
                    "type": "number",
                    "default": 5,
                    "displayOptions": { "show": { "resume": ["timeInterval"] } }
                },
                {
                    "displayName": "Wait Unit",
                    "name": "unit",
                    "type": "options",
                    "default": "seconds",
                    "displayOptions": { "show": { "resume": ["timeInterval"] } },
                    "options": [
                        { "name": "Seconds", "value": "seconds" },
                        { "name": "Minutes", "value": "minutes" },
                        { "name": "Hours", "value": "hours" }
                    ]
                },
                {
                    "displayName": "Suspends until POST /webhook-waiting/{executionId}.",
                    "name": "notice",
                    "type": "notice",
                    "default": "",
                    "displayOptions": { "show": { "resume": ["webhook"] } }
                }
            ],
            "icon": "fa:clock",
            "iconColor": "crimson",
            "codex": {
                "categories": ["Core Nodes"],
                "subcategories": { "Core Nodes": ["Flow"] }
            }
        }
    ])
}
