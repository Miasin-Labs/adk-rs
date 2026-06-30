//! SQLite-backed `SessionStore` and `ArtifactService`.
//!
//! Sessions and versioned artifacts are stored as JSON blobs in a SQLite
//! database, so a process can persist and reload state across runs (and across
//! store instances on the same file). Enabled by the default `sqlite` feature.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::artifact::{Artifact, ArtifactError, ArtifactService, ArtifactVersion};
use crate::event::Event;
use crate::ids::{AppName, ArtifactName, ArtifactVersionNumber, SessionId, UserId};
use crate::session::{Session, SessionError, SessionStore};

fn session_db(message: impl ToString) -> SessionError {
    SessionError::Db {
        message: message.to_string(),
    }
}

fn artifact_db(message: impl ToString) -> ArtifactError {
    ArtifactError::Db {
        message: message.to_string(),
    }
}

/// A SQLite-backed session store. Sessions persist as JSON keyed by session id.
#[derive(Clone)]
pub struct SqliteSessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSessionStore {
    /// Open (creating if needed) a session store at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let conn = Connection::open(path).map_err(session_db)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            [],
        )
        .map_err(session_db)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// An in-memory SQLite store (handy for tests).
    pub fn in_memory() -> Result<Self, SessionError> {
        Self::open(":memory:")
    }
}

impl SessionStore for SqliteSessionStore {
    fn create(&self, session: Session) -> Result<Session, SessionError> {
        self.save(session.clone())?;
        Ok(session)
    }

    fn load(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        let conn = self.conn.lock().map_err(|_| SessionError::Poisoned)?;
        let mut stmt = conn
            .prepare("SELECT data FROM sessions WHERE id = ?1")
            .map_err(session_db)?;
        let mut rows = stmt.query([id.as_str()]).map_err(session_db)?;
        match rows.next().map_err(session_db)? {
            Some(row) => {
                let data: String = row.get(0).map_err(session_db)?;
                let session = serde_json::from_str(&data)
                    .map_err(|source| SessionError::Json { source })?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    fn save(&self, session: Session) -> Result<(), SessionError> {
        let data =
            serde_json::to_string(&session).map_err(|source| SessionError::Json { source })?;
        let conn = self.conn.lock().map_err(|_| SessionError::Poisoned)?;
        conn.execute(
            "INSERT INTO sessions (id, data) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            rusqlite::params![session.id.as_str(), data],
        )
        .map_err(session_db)?;
        Ok(())
    }

    fn append_event(&self, id: &SessionId, event: Event) -> Result<Session, SessionError> {
        let mut session = self.load(id)?.unwrap_or_else(|| Session::new(id.clone()));
        session.append(event);
        self.save(session.clone())?;
        Ok(session)
    }
}

/// A SQLite-backed artifact service. Versioned artifacts persist as JSON keyed
/// by (app, user, session-or-global, name, version).
#[derive(Clone)]
pub struct SqliteArtifactService {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteArtifactService {
    /// Open (creating if needed) an artifact service at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArtifactError> {
        let conn = Connection::open(path).map_err(artifact_db)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS artifacts (
                app TEXT NOT NULL,
                user TEXT NOT NULL,
                session TEXT NOT NULL,
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (app, user, session, name, version)
            )",
            [],
        )
        .map_err(artifact_db)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// An in-memory artifact service (handy for tests).
    pub fn in_memory() -> Result<Self, ArtifactError> {
        Self::open(":memory:")
    }
}

fn session_key(session: Option<&SessionId>) -> &str {
    session.map(SessionId::as_str).unwrap_or("__global__")
}

impl ArtifactService for SqliteArtifactService {
    fn save_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: ArtifactName,
        bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<ArtifactVersion, ArtifactError> {
        let version = self
            .list_versions(app_name, user_id, session_id, &name)?
            .into_iter()
            .max()
            .map(ArtifactVersionNumber::next)
            .unwrap_or(ArtifactVersion::FIRST);
        let artifact = Artifact {
            name: name.clone(),
            version,
            bytes,
            mime_type,
        };
        let data =
            serde_json::to_string(&artifact).map_err(|source| ArtifactError::Json { source })?;
        let conn = self.conn.lock().map_err(|_| ArtifactError::Poisoned)?;
        conn.execute(
            "INSERT INTO artifacts (app, user, session, name, version, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                app_name.as_str(),
                user_id.as_str(),
                session_key(session_id),
                name.as_str(),
                version.0,
                data,
            ],
        )
        .map_err(artifact_db)?;
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
        let version = match version {
            Some(version) => version,
            None => match self
                .list_versions(app_name, user_id, session_id, name)?
                .into_iter()
                .max()
            {
                Some(version) => version,
                None => return Ok(None),
            },
        };
        let conn = self.conn.lock().map_err(|_| ArtifactError::Poisoned)?;
        let mut stmt = conn
            .prepare(
                "SELECT data FROM artifacts
                 WHERE app=?1 AND user=?2 AND session=?3 AND name=?4 AND version=?5",
            )
            .map_err(artifact_db)?;
        let mut rows = stmt
            .query(rusqlite::params![
                app_name.as_str(),
                user_id.as_str(),
                session_key(session_id),
                name.as_str(),
                version.0,
            ])
            .map_err(artifact_db)?;
        match rows.next().map_err(artifact_db)? {
            Some(row) => {
                let data: String = row.get(0).map_err(artifact_db)?;
                let artifact = serde_json::from_str(&data)
                    .map_err(|source| ArtifactError::Json { source })?;
                Ok(Some(artifact))
            }
            None => Ok(None),
        }
    }

    fn list_artifact_keys(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
    ) -> Result<Vec<ArtifactName>, ArtifactError> {
        let conn = self.conn.lock().map_err(|_| ArtifactError::Poisoned)?;
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT name FROM artifacts
                 WHERE app=?1 AND user=?2 AND session=?3 ORDER BY name",
            )
            .map_err(artifact_db)?;
        let names = stmt
            .query_map(
                rusqlite::params![
                    app_name.as_str(),
                    user_id.as_str(),
                    session_key(session_id),
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(artifact_db)?
            .filter_map(|name| name.ok())
            .filter_map(|name| ArtifactName::new(name).ok())
            .collect();
        Ok(names)
    }

    fn delete_artifact(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<(), ArtifactError> {
        let conn = self.conn.lock().map_err(|_| ArtifactError::Poisoned)?;
        conn.execute(
            "DELETE FROM artifacts WHERE app=?1 AND user=?2 AND session=?3 AND name=?4",
            rusqlite::params![
                app_name.as_str(),
                user_id.as_str(),
                session_key(session_id),
                name.as_str(),
            ],
        )
        .map_err(artifact_db)?;
        Ok(())
    }

    fn list_versions(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        session_id: Option<&SessionId>,
        name: &ArtifactName,
    ) -> Result<Vec<ArtifactVersion>, ArtifactError> {
        let conn = self.conn.lock().map_err(|_| ArtifactError::Poisoned)?;
        let mut stmt = conn
            .prepare(
                "SELECT version FROM artifacts
                 WHERE app=?1 AND user=?2 AND session=?3 AND name=?4 ORDER BY version",
            )
            .map_err(artifact_db)?;
        let versions = stmt
            .query_map(
                rusqlite::params![
                    app_name.as_str(),
                    user_id.as_str(),
                    session_key(session_id),
                    name.as_str(),
                ],
                |row| row.get::<_, u32>(0),
            )
            .map_err(artifact_db)?
            .filter_map(|version| version.ok())
            .map(ArtifactVersionNumber)
            .collect();
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_session_persists_and_reloads_across_instances_normal() {
        let dir = std::env::temp_dir().join(format!("adk-sqlite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sessions.db");
        let id = SessionId::new("s1").unwrap();

        {
            let store = SqliteSessionStore::open(&path).unwrap();
            let mut session = Session::new(id.clone());
            session.append(Event::text(
                crate::ids::InvocationId::new("i1").unwrap(),
                crate::event::EventAuthor::User,
                "remember me",
            ));
            store.save(session).unwrap();
        }
        // A fresh store on the same file must see the persisted session.
        let store = SqliteSessionStore::open(&path).unwrap();
        let loaded = store.load(&id).unwrap().expect("session persisted");
        assert_eq!(loaded.events.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sqlite_artifact_versions_increment_and_reload_normal() {
        let app = AppName::new("app").unwrap();
        let user = UserId::new("user").unwrap();
        let name = ArtifactName::new("report.txt").unwrap();
        let dir = std::env::temp_dir().join(format!("adk-sqlite-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("artifacts.db");

        let v1;
        let v2;
        {
            let svc = SqliteArtifactService::open(&path).unwrap();
            v1 = svc
                .save_artifact(&app, &user, None, name.clone(), b"one".to_vec(), "text/plain".into())
                .unwrap();
            v2 = svc
                .save_artifact(&app, &user, None, name.clone(), b"two".to_vec(), "text/plain".into())
                .unwrap();
        }
        assert!(v2 > v1);

        // Reload from a fresh instance on the same file.
        let svc = SqliteArtifactService::open(&path).unwrap();
        let versions = svc.list_versions(&app, &user, None, &name).unwrap();
        assert_eq!(versions.len(), 2);
        let latest = svc
            .load_artifact(&app, &user, None, &name, None)
            .unwrap()
            .expect("latest artifact");
        assert_eq!(latest.bytes, b"two".to_vec());
        let keys = svc.list_artifact_keys(&app, &user, None).unwrap();
        assert_eq!(keys, vec![name.clone()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
