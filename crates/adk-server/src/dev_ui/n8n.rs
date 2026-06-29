//! n8n editor-ui compatibility surface.
//!
//! The dev server ships the real, verbatim n8n editor-ui (a Vue 3 SPA, built
//! from the upstream `n8n` workspace and staged under `static/n8n-editor-ui`).
//! That SPA boots against n8n's REST contract, so this module implements just
//! enough of `/rest/*` and `/types/*` for the editor to reach its main canvas
//! with no signin/setup wall, plus a no-op SSE push channel and an SPA-aware
//! fallback for client-side routes.
//!
//! Response conventions (verified against `@n8n/rest-api-client`):
//! - every `/rest/*` JSON body is wrapped in a `{ "data": ... }` envelope
//!   (`makeRestApiRequest` unwraps `.data`);
//! - `/types/*.json` are fetched raw and return bare arrays (no envelope).

use std::path::PathBuf;
use std::sync::OnceLock;

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

use super::DevUiState;

pub(crate) mod credentials;
pub(crate) mod expr;
pub(crate) mod icons;
pub(crate) mod nodes;
pub(crate) mod push;
pub(crate) mod run;
pub(crate) mod workflows;

/// Current unix time in milliseconds (n8n timestamps are ms).
fn now_ms() -> u64 {
    super::tools::now_unix_ms()
}

/// A throwaway version id stamped onto saved workflows.
fn version_id() -> String {
    format!("v-{:x}", now_ms())
}

/// Next free numeric id given existing `<prefix><n>` ids — keeps the in-memory
/// counter from colliding with persisted entries after a restart.
fn next_id_after<'a>(ids: impl Iterator<Item = &'a String>, prefix: &str) -> u64 {
    ids.filter_map(|id| id.strip_prefix(prefix))
        .filter_map(|suffix| suffix.parse::<u64>().ok())
        .max()
        .map_or(1, |max| max + 1)
}

/// Format the current time as an ISO-8601 UTC string.
fn iso_now() -> String {
    super::tools::now_iso()
}

/// Directory holding the preprocessed n8n editor-ui dist (see
/// `scripts/prepare-n8n-ui.mjs`). Served at the site root.
pub const N8N_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/static/n8n-editor-ui");

/// Stable owner identity handed back for every auth probe — the editor runs as
/// a single local owner, so there is never a login or first-run setup wall.
fn owner_user() -> Value {
    json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "email": "admin@adk-rs.local",
        "firstName": "ADK",
        "lastName": "Owner",
        "role": "global:owner",
        "isPending": false,
        "isOwner": true,
        "mfaEnabled": false,
        "settings": { "userActivated": true },
        "globalScopes": [
            "workflow:create", "workflow:read", "workflow:update", "workflow:delete",
            "workflow:list", "workflow:execute", "workflow:share",
            "credential:create", "credential:read", "credential:update", "credential:delete",
            "credential:list", "credential:share",
            "project:create", "project:read", "project:update", "project:list",
            "tag:create", "tag:read", "tag:update", "tag:list",
            "variable:create", "variable:read", "variable:update", "variable:list",
            "user:read", "user:list"
        ],
        "featureFlags": {}
    })
}

fn personal_project() -> Value {
    json!({
        "id": "proj-personal",
        "name": "Personal",
        "type": "personal",
        "icon": null,
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z",
        "role": "project:personalOwner",
        "scopes": ["workflow:create", "workflow:read", "credential:create", "credential:read"]
    })
}

/// The FrontendSettings payload. Fields are chosen so the editor boots as an
/// authenticated owner with no walls, SSE push, and all outbound integrations
/// (telemetry, posthog, templates, version checks) disabled for offline use.
fn frontend_settings() -> Value {
    json!({
        "settingsMode": "authenticated",
        "inE2ETests": false,
        "previewMode": false,
        "defaultLocale": "en",
        "authCookie": { "secure": false },
        "endpointForm": "form",
        "endpointFormTest": "form-test",
        "endpointFormWaiting": "form-waiting",
        "endpointMcp": "mcp",
        "endpointMcpTest": "mcp-test",
        "endpointWebhook": "webhook",
        "endpointWebhookTest": "webhook-test",
        "endpointWebhookWaiting": "webhook-waiting",
        "endpointHealth": "healthz",
        "urlBaseWebhook": "http://localhost:8091/",
        "urlBaseEditor": "http://localhost:8091/",
        "saveDataErrorExecution": "all",
        "saveDataSuccessExecution": "all",
        "saveManualExecutions": true,
        "saveExecutionProgress": false,
        "executionTimeout": -1,
        "maxExecutionTimeout": 3600,
        "workflowCallerPolicyDefaultOption": "workflowsFromSameOwner",
        "timezone": "UTC",
        "instanceId": "adk-rs-instance",
        "versionCli": "1.0.0",
        "nodeJsVersion": "20.0.0",
        "nodeEnv": "production",
        "concurrency": -1,
        "evaluationConcurrencyLimit": 1,
        "releaseChannel": "stable",
        "binaryDataMode": "default",
        "databaseType": "sqlite",
        "isDocker": false,
        "pushBackend": "sse",
        "jwksUri": "",
        "oauthCallbackUrls": {
            "oauth1": "http://localhost:8091/rest/oauth1-credential/callback",
            "oauth2": "http://localhost:8091/rest/oauth2-credential/callback"
        },
        "n8nMetadata": {},
        "deployment": { "type": "default" },
        "personalizationSurveyEnabled": false,
        "hiringBannerEnabled": false,
        "workflowTagsDisabled": false,
        "easyAIWorkflowOnboarded": true,
        "userManagement": {
            "quota": -1,
            "showSetupOnFirstLoad": false,
            "smtpSetup": false,
            "authenticationMethod": "email",
            "passwordMinLength": 8
        },
        "sso": {
            "saml": { "loginEnabled": false, "loginLabel": "" },
            "ldap": { "loginEnabled": false, "loginLabel": "" },
            "oidc": { "loginEnabled": false, "loginUrl": "", "callbackUrl": "" },
            "managedByEnv": false
        },
        "mfa": { "enabled": false, "enforced": false },
        "folders": { "enabled": false },
        "publicApi": { "enabled": false, "latestVersion": 1, "path": "api", "swaggerUi": { "enabled": false } },
        "communityNodesEnabled": false,
        "unverifiedCommunityNodesEnabled": false,
        "communityNodesManagedByEnv": false,
        "aiAssistant": { "enabled": false, "setup": false },
        "askAi": { "enabled": false },
        "aiBuilder": { "enabled": false, "setup": false },
        "aiCredits": { "enabled": false, "credits": 0, "setup": false },
        "ai": { "allowSendingParameterValues": true },
        "variables": { "limit": -1 },
        "dataTables": { "maxSize": 52428800 },
        "banners": { "dismissed": [] },
        "activeModules": [],
        "allowedModules": { "builtIn": [], "external": [] },
        "templates": { "enabled": false, "host": "" },
        "telemetry": { "enabled": false },
        "posthog": {
            "enabled": false, "apiHost": "", "apiKey": "",
            "autocapture": false, "disableSessionRecording": true, "debug": false, "proxy": ""
        },
        "logLevel": "info",
        "hideUsagePage": true,
        "license": {
            "planName": "Community",
            "consumerId": "unknown",
            "environment": "production"
        },
        "security": { "blockFileAccessToN8nFiles": true },
        "workflowHistory": { "pruneTime": -1, "licensePruneTime": -1 },
        "pruning": { "isEnabled": false, "maxAge": 336, "maxCount": 10000 },
        "versionNotifications": {
            "enabled": false, "endpoint": "", "whatsNewEnabled": false,
            "whatsNewEndpoint": "", "infoUrl": ""
        },
        "dynamicBanners": { "endpoint": "", "filters": { "publishedWorkflowCount": 0 } },
        "enterprise": {
            "sharing": false, "ldap": false, "saml": false, "oidc": false,
            "mfaEnforcement": false, "logStreaming": false, "advancedExecutionFilters": false,
            "variables": false, "sourceControl": false, "auditLogs": false,
            "externalSecrets": false, "showNonProdBanner": false, "debugInEditor": false,
            "binaryDataS3": false, "workerView": false, "advancedPermissions": false,
            "workflowDiffs": false, "namedVersions": false, "provisioning": false,
            "projects": { "team": { "limit": 0 } },
            "customRoles": false, "personalSpacePolicy": false,
            "dataRedaction": false, "otelCustomSpanAttributes": false
        }
    })
}

// ---- handlers -----------------------------------------------------------

fn data(value: Value) -> Json<Value> {
    Json(json!({ "data": value }))
}

async fn settings() -> Json<Value> {
    data(frontend_settings())
}

async fn login() -> Json<Value> {
    data(owner_user())
}

async fn module_settings() -> Json<Value> {
    data(json!({}))
}

async fn projects_my() -> Json<Value> {
    data(json!([personal_project()]))
}

async fn projects_personal() -> Json<Value> {
    let mut project = personal_project();
    project["relations"] = json!([]);
    data(project)
}

async fn projects_count() -> Json<Value> {
    data(json!({ "personal": 1, "team": 0, "total": 1 }))
}

async fn users_list() -> Json<Value> {
    // The users store fetches a paginated `UsersList` (`{ count, items }`) and
    // destructures `items` — a bare array would make it `undefined.forEach`.
    data(json!({ "count": 1, "items": [owner_user()] }))
}

async fn roles() -> Json<Value> {
    data(json!({
        "global": [
            { "slug": "global:owner", "role": "global:owner", "scopes": [], "licensed": true },
            { "slug": "global:admin", "role": "global:admin", "scopes": [], "licensed": false },
            { "slug": "global:member", "role": "global:member", "scopes": [], "licensed": true }
        ],
        "project": [],
        "credential": [],
        "workflow": []
    }))
}

async fn workflow_new() -> Json<Value> {
    data(json!({
        "name": "My workflow",
        "onboardingFlowEnabled": false,
        "settings": {
            "timezone": "DEFAULT",
            "saveDataErrorExecution": "all",
            "saveDataSuccessExecution": "all",
            "saveManualExecutions": true,
            "saveExecutionProgress": false,
            "executionTimeout": -1,
            "executionOrder": "v1"
        }
    }))
}

/// Empty `{ "data": [] }` list stub.
async fn empty_list() -> Json<Value> {
    data(json!([]))
}

/// Empty `{ "data": {} }` object stub.
async fn empty_object() -> Json<Value> {
    data(json!({}))
}

/// Safety net for any `/rest/*` boot call not explicitly modelled: return a
/// valid-but-empty envelope so the SPA's JSON parser never chokes on an HTML
/// 404 body. Registered as the `/rest` sub-tree catch-all.
async fn rest_catchall(Path(rest): Path<String>) -> Response {
    // Bare-list endpoints are far more common than object endpoints at boot;
    // an empty object is the safer default for an unknown shape.
    let _ = rest;
    data(json!({})).into_response()
}

/// Router for the n8n REST + types + push surface. Merged into the dev_ui
/// router; the SPA static serving + fallback is wired in `routes.rs`.
pub fn router() -> Router<DevUiState> {
    Router::new()
        // boot-critical
        .route("/rest/settings", get(settings))
        .route("/rest/login", get(login).post(login))
        .route("/rest/logout", axum::routing::post(login))
        .route("/rest/module-settings", get(module_settings))
        .route("/rest/projects/my-projects", get(projects_my))
        .route("/rest/projects/personal", get(projects_personal))
        .route("/rest/projects/count", get(projects_count))
        // node catalog + branded node icons
        .route("/types/nodes.json", get(nodes::catalog))
        .route("/adk-icons/{name}", get(icons::serve))
        .route("/types/credentials.json", get(credentials::credential_types))
        .route("/rest/node-types", post(nodes::node_types))
        // push (pushRef-keyed SSE)
        .route("/rest/push", get(push::push))
        // workflow store + run
        .route("/rest/workflows", get(workflows::list).post(workflows::create))
        .route("/rest/workflows/new", get(workflow_new))
        .route(
            "/rest/workflows/{id}",
            get(workflows::get_one)
                .patch(workflows::update)
                .delete(workflows::delete),
        )
        .route("/rest/workflows/{id}/run", post(run::run_workflow))
        .route("/rest/executions/{id}", get(workflows::get_execution))
        // resume a suspended (Wait node) execution — human-in-the-loop
        .route(
            "/webhook-waiting/{execution_id}",
            get(run::resume_webhook).post(run::resume_webhook),
        )
        // main-view stubs
        .route("/rest/active-workflows", get(empty_list))
        .route("/rest/credentials", get(credentials::list).post(credentials::create))
        .route("/rest/credentials/new", get(credentials::new_name))
        .route("/rest/credentials/for-workflow", get(credentials::for_workflow))
        .route(
            "/rest/credentials/{id}",
            get(credentials::get_one)
                .patch(credentials::update)
                .delete(credentials::delete),
        )
        .route("/rest/credential-types", get(empty_list))
        .route("/rest/tags", get(empty_list))
        .route("/rest/annotation-tags", get(empty_list))
        .route("/rest/variables", get(empty_list))
        .route("/rest/users", get(users_list))
        .route("/rest/favorites", get(empty_list))
        .route("/rest/roles", get(roles))
        .route("/rest/node-translation-headers", get(empty_object))
        .route("/rest/events/session-started", get(empty_object))
        .route("/rest/dynamic-banners", get(empty_object))
        .route("/rest/settings/{*sub}", get(empty_object))
        // catch-all for any other /rest call fired during boot
        .route("/rest/{*rest}", get(rest_catchall).post(rest_catchall))
}

/// Static file routes for the built SPA: the hashed `/assets/*` bundles, the
/// `/static/*` runtime assets, the favicon, and the lazily-loaded tree-sitter
/// wasm. Everything else (the SPA shell + stray API calls) is handled by
/// [`spa_fallback`], wired as the router-level fallback.
pub fn static_routes() -> Router<DevUiState> {
    let file = |name: &str| ServeFile::new(format!("{N8N_DIR}/{name}"));
    Router::new()
        .nest_service("/assets", ServeDir::new(format!("{N8N_DIR}/assets")))
        .nest_service("/static", ServeDir::new(format!("{N8N_DIR}/static")))
        .route_service("/favicon.ico", file("favicon.ico"))
        .route_service("/tree-sitter.wasm", file("tree-sitter.wasm"))
        .route_service("/tree-sitter-bash.wasm", file("tree-sitter-bash.wasm"))
}

/// SPA-aware fallback (wired as the router-level `.fallback`): anything not
/// matched by an API route or a static mount lands here. Stray `/rest` &
/// `/types` calls get an empty-but-valid JSON body (never an HTML 404 the SPA's
/// JSON parser would choke on); every other path is a client-side route and
/// receives the SPA shell with a 200 so deep links and reloads work.
pub async fn spa_fallback(uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/rest/") {
        return data(json!({})).into_response();
    }
    if path.starts_with("/types/") {
        return Json(json!([])).into_response();
    }
    match index_html() {
        Some(html) => Html(html.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "n8n editor-ui not built").into_response(),
    }
}

fn index_html() -> Option<&'static String> {
    static INDEX: OnceLock<Option<String>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            let path = PathBuf::from(N8N_DIR).join("index.html");
            std::fs::read_to_string(path).ok()
        })
        .as_ref()
}
