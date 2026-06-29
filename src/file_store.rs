use std::fs;
use std::path::{Path, PathBuf};

use crate::artifact::{Artifact, ArtifactError, ArtifactService, ArtifactVersion};
use crate::eval::{EvalCase, EvalError, EvalResult, EvalService};
use crate::event::Event;
use crate::ids::{AppName, ArtifactName, ArtifactVersionNumber, SessionId, UserId};
use crate::session::{Session, SessionError, SessionStore};

#[derive(Debug, Clone)]
pub struct FileSessionStore {
    root: PathBuf,
}

impl FileSessionStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().join("sessions"),
        }
    }

    fn path(&self, id: &SessionId) -> PathBuf {
        self.root
            .join(format!("{}.json", safe_segment(id.as_str())))
    }
}

impl SessionStore for FileSessionStore {
    fn create(&self, session: Session) -> Result<Session, SessionError> {
        self.save(session.clone())?;
        Ok(session)
    }

    fn load(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        let path = self.path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path).map_err(|source| SessionError::Io { source })?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| SessionError::Json { source })
    }

    fn save(&self, session: Session) -> Result<(), SessionError> {
        fs::create_dir_all(&self.root).map_err(|source| SessionError::Io { source })?;
        let bytes =
            serde_json::to_vec_pretty(&session).map_err(|source| SessionError::Json { source })?;
        fs::write(self.path(&session.id), bytes).map_err(|source| SessionError::Io { source })
    }

    fn append_event(&self, id: &SessionId, event: Event) -> Result<Session, SessionError> {
        let mut session = self.load(id)?.unwrap_or_else(|| Session::new(id.clone()));
        session.append(event);
        self.save(session.clone())?;
        Ok(session)
    }
}

#[derive(Debug, Clone)]
pub struct FileArtifactService {
    root: PathBuf,
}

impl FileArtifactService {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().join("artifacts"),
        }
    }

    fn dir(
        &self,
        app: &AppName,
        user: &UserId,
        session: Option<&SessionId>,
        name: &ArtifactName,
    ) -> PathBuf {
        self.root
            .join(safe_segment(app.as_str()))
            .join(safe_segment(user.as_str()))
            .join(safe_segment(
                session.map(SessionId::as_str).unwrap_or("global"),
            ))
            .join(safe_segment(name.as_str()))
    }
}

impl ArtifactService for FileArtifactService {
    fn save_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: ArtifactName,
        bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<ArtifactVersion, ArtifactError> {
        let dir = self.dir(app_name, user_id, session_id, &name);
        fs::create_dir_all(&dir).map_err(|source| ArtifactError::Io { source })?;
        let version = self
            .list_versions(app_name, user_id, session_id, &name)?
            .into_iter()
            .max()
            .map(ArtifactVersionNumber::next)
            .unwrap_or(ArtifactVersion::FIRST);
        let artifact = Artifact {
            name,
            version,
            bytes,
            mime_type,
        };
        let data = serde_json::to_vec_pretty(&artifact)
            .map_err(|source| ArtifactError::Json { source })?;
        fs::write(dir.join(format!("{}.json", version.0)), data)
            .map_err(|source| ArtifactError::Io { source })?;
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
        let Some(version) = version.or_else(|| {
            self.list_versions(app_name, user_id, session_id, name)
                .ok()
                .and_then(|versions| versions.into_iter().max())
        }) else {
            return Ok(None);
        };
        let path = self
            .dir(app_name, user_id, session_id, name)
            .join(format!("{}.json", version.0));
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path).map_err(|source| ArtifactError::Io { source })?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| ArtifactError::Json { source })
    }

    fn list_artifact_keys(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
    ) -> Result<Vec<ArtifactName>, ArtifactError> {
        let dir = self
            .root
            .join(safe_segment(app_name.as_str()))
            .join(safe_segment(user_id.as_str()))
            .join(safe_segment(
                session_id.map(SessionId::as_str).unwrap_or("global"),
            ));
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let keys = fs::read_dir(dir)
            .map_err(|source| ArtifactError::Io { source })?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| ArtifactName::new(name).ok())
            .collect::<Vec<_>>();
        Ok(keys)
    }

    fn delete_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<(), ArtifactError> {
        let dir = self.dir(app_name, user_id, session_id, name);
        if dir.exists() {
            fs::remove_dir_all(dir).map_err(|source| ArtifactError::Io { source })?;
        }
        Ok(())
    }

    fn list_versions(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<Vec<ArtifactVersion>, ArtifactError> {
        let dir = self.dir(app_name, user_id, session_id, name);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut versions = fs::read_dir(dir)
            .map_err(|source| ArtifactError::Io { source })?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.path().file_stem()?.to_str()?.parse::<u32>().ok())
            .map(ArtifactVersionNumber)
            .collect::<Vec<_>>();
        versions.sort();
        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct FileEvalService {
    root: PathBuf,
}

impl FileEvalService {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().join("eval"),
        }
    }

    fn cases_path(&self) -> PathBuf {
        self.root.join("cases.json")
    }

    fn results_path(&self) -> PathBuf {
        self.root.join("results.json")
    }
}

impl EvalService for FileEvalService {
    fn put_case(&self, case: EvalCase) -> Result<(), EvalError> {
        let mut cases = read_json::<Vec<EvalCase>>(&self.cases_path())?.unwrap_or_default();
        cases.retain(|stored| stored.id != case.id);
        cases.push(case);
        write_json(&self.root, &self.cases_path(), &cases)
    }

    fn list_cases(&self) -> Result<Vec<EvalCase>, EvalError> {
        Ok(read_json(&self.cases_path())?.unwrap_or_default())
    }

    fn record_result(&self, result: EvalResult) -> Result<(), EvalError> {
        let mut results = read_json::<Vec<EvalResult>>(&self.results_path())?.unwrap_or_default();
        results.push(result);
        write_json(&self.root, &self.results_path(), &results)
    }

    fn list_results(&self, case_id: &str) -> Result<Vec<EvalResult>, EvalError> {
        Ok(read_json::<Vec<EvalResult>>(&self.results_path())?
            .unwrap_or_default()
            .into_iter()
            .filter(|result| result.case_id == case_id)
            .collect())
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, EvalError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|source| EvalError::Io { source })?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|source| EvalError::Json { source })
}

fn write_json<T: serde::Serialize>(root: &Path, path: &Path, value: &T) -> Result<(), EvalError> {
    fs::create_dir_all(root).map_err(|source| EvalError::Io { source })?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| EvalError::Json { source })?;
    fs::write(path, bytes).map_err(|source| EvalError::Io { source })
}

fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            other => other,
        })
        .collect()
}
