use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudTarget {
    VertexAi,
    Gcs,
    Database,
    CloudRun,
    Gke,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudCredential {
    pub project_id: String,
    pub region: String,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    pub target: CloudTarget,
    pub service_name: String,
    pub project_id: String,
    pub region: String,
    pub steps: Vec<String>,
}

pub trait DeploymentBackend: Send + Sync {
    fn plan_deploy(
        &self,
        target: CloudTarget,
        service_name: &str,
    ) -> Result<DeploymentPlan, DeploymentError>;
}

#[derive(Debug, Clone)]
pub struct ConfiguredCloudBackend {
    credential: CloudCredential,
}

impl ConfiguredCloudBackend {
    pub fn new(credential: CloudCredential) -> Self {
        Self { credential }
    }
}

impl DeploymentBackend for ConfiguredCloudBackend {
    fn plan_deploy(
        &self,
        target: CloudTarget,
        service_name: &str,
    ) -> Result<DeploymentPlan, DeploymentError> {
        if self.credential.bearer_token.is_none() {
            return Err(DeploymentError::MissingCredential);
        }
        let label = target.label();
        Ok(DeploymentPlan {
            target,
            service_name: service_name.to_owned(),
            project_id: self.credential.project_id.clone(),
            region: self.credential.region.clone(),
            steps: vec![
                format!("Resolve credentials for {label}"),
                format!("Package {service_name}"),
                format!("Deploy {service_name} to {label}"),
            ],
        })
    }
}

impl CloudTarget {
    fn label(&self) -> &'static str {
        match self {
            Self::VertexAi => "Vertex AI",
            Self::Gcs => "GCS",
            Self::Database => "database",
            Self::CloudRun => "Cloud Run",
            Self::Gke => "GKE",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeploymentError {
    #[error("cloud credential bearer token is required")]
    MissingCredential,
}
