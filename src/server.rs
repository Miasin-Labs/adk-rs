use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevServerConfig {
    pub host: String,
    pub port: u16,
    pub hot_reload: bool,
}

impl Default for DevServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 8080,
            hot_reload: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiRoute {
    Apps,
    Sessions,
    Events,
    Artifacts,
    EvalSets,
    Builder,
    Recordings,
    Metrics,
    DeployPlan,
    Run,
    RunSse,
    Live,
}

impl ApiRoute {
    pub fn path(self) -> &'static str {
        match self {
            Self::Apps => "/apps",
            Self::Sessions => "/sessions",
            Self::Events => "/events",
            Self::Artifacts => "/artifacts",
            Self::EvalSets => "/eval_sets",
            Self::Builder => "/builder",
            Self::Recordings => "/recordings",
            Self::Metrics => "/metrics",
            Self::DeployPlan => "/deploy/plan",
            Self::Run => "/run",
            Self::RunSse => "/run_sse",
            Self::Live => "/live",
        }
    }
}
