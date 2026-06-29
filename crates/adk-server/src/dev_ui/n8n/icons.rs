//! Branded SVG icons for the ADK-specific nodes, served at `/adk-icons/*`.
//!
//! Node descriptions set `iconUrl: "adk-icons/<name>.svg"`; the editor resolves
//! that against the base URL (`prefixBaseUrl`) and renders it as an `<img>`,
//! taking precedence over the Lucide `icon` fallback.

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

pub(crate) async fn serve(Path(name): Path<String>) -> Response {
    let svg = match name.as_str() {
        "agent.svg" => AGENT,
        "subagent.svg" => SUBAGENT,
        "memory.svg" => MEMORY,
        "http.svg" => HTTP,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        svg,
    )
        .into_response()
}

const AGENT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#1f9c8a" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="4.5" y="8" width="15" height="11" rx="2.5"/><path d="M12 8V5.5"/><circle cx="12" cy="4" r="1.3"/><path d="M2.5 13.5v2M21.5 13.5v2"/><circle cx="9" cy="13" r="1.3" fill="#1f9c8a" stroke="none"/><circle cx="15" cy="13" r="1.3" fill="#1f9c8a" stroke="none"/><path d="M9.5 16.5h5"/></svg>"##;

const SUBAGENT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#6b4fbb" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="8" y="3" width="8" height="6" rx="1.5"/><rect x="2.5" y="15" width="7" height="6" rx="1.5"/><rect x="14.5" y="15" width="7" height="6" rx="1.5"/><path d="M12 9v3M12 12H6v3M12 12h6v3"/></svg>"##;

const MEMORY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#d97706" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5.5" rx="7" ry="2.8"/><path d="M5 5.5v13c0 1.5 3.1 2.8 7 2.8s7-1.3 7-2.8v-13"/><path d="M5 12c0 1.5 3.1 2.8 7 2.8s7-1.3 7-2.8"/></svg>"##;

const HTTP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#2233dd" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17"/><path d="M12 3.5c2.3 2.3 3.5 5.3 3.5 8.5s-1.2 6.2-3.5 8.5c-2.3-2.3-3.5-5.3-3.5-8.5S9.7 5.8 12 3.5z"/></svg>"##;
