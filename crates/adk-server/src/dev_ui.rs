mod agent;
mod artifacts;
mod config;
mod evals;
mod events;
mod graph;
mod openai;
mod routes;
mod state;
mod tests;
mod tools;
mod traces;
mod types;

pub use state::DevUiState;

pub fn router(state: DevUiState) -> axum::Router {
    routes::router(state)
}
