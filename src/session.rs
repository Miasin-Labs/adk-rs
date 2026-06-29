use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::Event;
use crate::ids::{AppName, EventId, SessionId, StateKey, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub app_name: AppName,
    pub user_id: UserId,
    pub state: BTreeMap<StateKey, Value>,
    pub events: Vec<Event>,
    pub last_update_time: u64,
}

impl Session {
    pub fn new(id: SessionId) -> Self {
        Self::for_user(
            AppName::trusted("default_app"),
            UserId::trusted("default_user"),
            id,
        )
    }

    pub fn for_user(app_name: AppName, user_id: UserId, id: SessionId) -> Self {
        Self {
            app_name,
            user_id,
            id,
            state: BTreeMap::new(),
            events: Vec::new(),
            last_update_time: 0,
        }
    }

    pub fn append(&mut self, mut event: Event) {
        event.id = EventId::for_index(self.events.len() + 1);
        for (key, value) in &event.actions.state_delta {
            self.state.insert(key.clone(), value.clone());
        }
        self.last_update_time = event.timestamp_seconds;
        self.events.push(event);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session store lock poisoned")]
    Poisoned,
    #[error("session store I/O failed")]
    Io { source: std::io::Error },
    #[error("session store JSON failed")]
    Json { source: serde_json::Error },
}

pub trait SessionStore: Send + Sync {
    fn create(&self, session: Session) -> Result<Session, SessionError>;
    fn load(&self, id: &SessionId) -> Result<Option<Session>, SessionError>;
    fn save(&self, session: Session) -> Result<(), SessionError>;
    fn append_event(&self, id: &SessionId, event: Event) -> Result<Session, SessionError>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<Mutex<HashMap<SessionId, Session>>>,
}

impl SessionStore for InMemorySessionStore {
    fn create(&self, session: Session) -> Result<Session, SessionError> {
        self.save(session.clone())?;
        Ok(session)
    }

    fn load(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        let guard = self.sessions.lock().map_err(|_| SessionError::Poisoned)?;
        Ok(guard.get(id).cloned())
    }

    fn save(&self, session: Session) -> Result<(), SessionError> {
        let mut guard = self.sessions.lock().map_err(|_| SessionError::Poisoned)?;
        guard.insert(session.id.clone(), session);
        Ok(())
    }

    fn append_event(&self, id: &SessionId, event: Event) -> Result<Session, SessionError> {
        let mut guard = self.sessions.lock().map_err(|_| SessionError::Poisoned)?;
        let mut session = guard
            .get(id)
            .cloned()
            .unwrap_or_else(|| Session::new(id.clone()));
        session.append(event);
        guard.insert(id.clone(), session.clone());
        Ok(session)
    }
}
