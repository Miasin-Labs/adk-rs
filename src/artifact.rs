use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ids::{AppName, ArtifactName, ArtifactVersionNumber, SessionId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub name: ArtifactName,
    pub version: ArtifactVersionNumber,
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

pub type ArtifactVersion = ArtifactVersionNumber;

pub trait ArtifactService: Send + Sync {
    fn save_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: ArtifactName,
        bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<ArtifactVersion, ArtifactError>;

    fn load_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
        version: Option<ArtifactVersion>,
    ) -> Result<Option<Artifact>, ArtifactError>;

    fn list_artifact_keys(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
    ) -> Result<Vec<ArtifactName>, ArtifactError>;

    fn delete_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<(), ArtifactError>;

    fn list_versions(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<Vec<ArtifactVersion>, ArtifactError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("artifact store lock poisoned")]
    Poisoned,
    #[error("artifact store I/O failed")]
    Io { source: std::io::Error },
    #[error("artifact store JSON failed")]
    Json { source: serde_json::Error },
    #[error("artifact store database error: {message}")]
    Db { message: String },
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryArtifactService {
    inner: Arc<Mutex<BTreeMap<String, Artifact>>>,
}

impl ArtifactService for InMemoryArtifactService {
    fn save_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: ArtifactName,
        bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<ArtifactVersion, ArtifactError> {
        let mut guard = self.inner.lock().map_err(|_| ArtifactError::Poisoned)?;
        let version = next_version(&guard, app_name, user_id, session_id, &name);
        let artifact = Artifact {
            name: name.clone(),
            version,
            bytes,
            mime_type,
        };
        guard.insert(key(app_name, user_id, session_id, &name, version), artifact);
        Ok(version)
    }

    fn load_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
        version: Option<ArtifactVersion>,
    ) -> Result<Option<Artifact>, ArtifactError> {
        let guard = self.inner.lock().map_err(|_| ArtifactError::Poisoned)?;
        let Some(version) =
            version.or_else(|| latest_version(&guard, app_name, user_id, session_id, name))
        else {
            return Ok(None);
        };
        Ok(guard
            .get(&key(app_name, user_id, session_id, name, version))
            .cloned())
    }

    fn list_artifact_keys(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
    ) -> Result<Vec<ArtifactName>, ArtifactError> {
        let guard = self.inner.lock().map_err(|_| ArtifactError::Poisoned)?;
        Ok(guard
            .keys()
            .filter_map(|stored| stored.strip_prefix(&scope_prefix(app_name, user_id, session_id)))
            .filter_map(|tail| tail.split_once('@').map(|(name, _)| name))
            .filter_map(|name| ArtifactName::new(name.to_owned()).ok())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    fn delete_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<(), ArtifactError> {
        let mut guard = self.inner.lock().map_err(|_| ArtifactError::Poisoned)?;
        let prefix = format!(
            "{}{}@",
            scope_prefix(app_name, user_id, session_id),
            name.as_str()
        );
        guard.retain(|stored, _| !stored.starts_with(&prefix));
        Ok(())
    }

    fn list_versions(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<Vec<ArtifactVersion>, ArtifactError> {
        let guard = self.inner.lock().map_err(|_| ArtifactError::Poisoned)?;
        Ok(version_numbers(&guard, app_name, user_id, session_id, name)
            .into_iter()
            .map(ArtifactVersionNumber)
            .collect())
    }
}

fn next_version(
    artifacts: &BTreeMap<String, Artifact>,
    app_name: &AppName,
    user_id: &UserId,
    session_id: Option<&SessionId>,
    name: &ArtifactName,
) -> ArtifactVersion {
    latest_version(artifacts, app_name, user_id, session_id, name)
        .map(ArtifactVersionNumber::next)
        .unwrap_or(ArtifactVersion::FIRST)
}

fn latest_version(
    artifacts: &BTreeMap<String, Artifact>,
    app_name: &AppName,
    user_id: &UserId,
    session_id: Option<&SessionId>,
    name: &ArtifactName,
) -> Option<ArtifactVersion> {
    version_numbers(artifacts, app_name, user_id, session_id, name)
        .into_iter()
        .max()
        .map(ArtifactVersionNumber)
}

fn version_numbers(
    artifacts: &BTreeMap<String, Artifact>,
    app_name: &AppName,
    user_id: &UserId,
    session_id: Option<&SessionId>,
    name: &ArtifactName,
) -> Vec<u32> {
    let prefix = format!(
        "{}{}@",
        scope_prefix(app_name, user_id, session_id),
        name.as_str()
    );
    artifacts
        .keys()
        .filter_map(|stored| stored.strip_prefix(&prefix))
        .filter_map(|version| version.parse::<u32>().ok())
        .collect()
}

fn key(
    app_name: &AppName,
    user_id: &UserId,
    session_id: Option<&SessionId>,
    name: &ArtifactName,
    version: ArtifactVersion,
) -> String {
    format!(
        "{}{}",
        scope_prefix(app_name, user_id, session_id),
        name.version_key(version)
    )
}

fn scope_prefix(app_name: &AppName, user_id: &UserId, session_id: Option<&SessionId>) -> String {
    let session_id = session_id.map(SessionId::as_str).unwrap_or("global");
    format!("{}:{}:{}:", app_name.as_str(), user_id.as_str(), session_id)
}
