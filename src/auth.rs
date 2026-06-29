use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ids::{AppName, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthScheme {
    ApiKey {
        header: String,
    },
    HttpBearer,
    OAuth2 {
        authorization_url: String,
        token_url: String,
        scopes: Vec<String>,
    },
    OpenIdConnect {
        discovery_url: String,
    },
    ServiceAccount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthCredential {
    ApiKey(String),
    BearerToken(String),
    OAuth2 {
        access_token: String,
        refresh_token: Option<String>,
        expires_at_epoch: Option<u64>,
    },
    ServiceAccountJson(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthConfig {
    pub scheme: AuthScheme,
    pub credential_key: String,
    pub raw_credential: Option<AuthCredential>,
    pub exchanged_credential: Option<AuthCredential>,
}

impl AuthConfig {
    pub fn stable_key(tool_name: &str, function_call_id: &str) -> String {
        format!("{tool_name}:{function_call_id}")
    }
}

pub struct CredentialManager<S: CredentialService> {
    service: S,
}

impl<S: CredentialService> CredentialManager<S> {
    pub fn new(service: S) -> Self {
        Self { service }
    }

    pub fn resolve(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        config: &AuthConfig,
    ) -> Result<Option<AuthCredential>, AuthError> {
        if let Some(credential) = &config.exchanged_credential {
            return Ok(Some(credential.clone()));
        }
        if let Some(credential) = &config.raw_credential {
            return Ok(Some(credential.clone()));
        }
        self.service
            .get_credential(app_name, user_id, &config.credential_key)
    }
}

pub trait CredentialService: Send + Sync {
    fn put_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
        credential: AuthCredential,
    ) -> Result<(), AuthError>;

    fn get_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
    ) -> Result<Option<AuthCredential>, AuthError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("credential store lock poisoned")]
    Poisoned,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryCredentialService {
    credentials: Arc<Mutex<BTreeMap<String, AuthCredential>>>,
}

impl CredentialService for InMemoryCredentialService {
    fn put_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
        credential: AuthCredential,
    ) -> Result<(), AuthError> {
        let mut guard = self.credentials.lock().map_err(|_| AuthError::Poisoned)?;
        guard.insert(credential_key(app_name, user_id, key), credential);
        Ok(())
    }

    fn get_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
    ) -> Result<Option<AuthCredential>, AuthError> {
        let guard = self.credentials.lock().map_err(|_| AuthError::Poisoned)?;
        Ok(guard.get(&credential_key(app_name, user_id, key)).cloned())
    }
}

fn credential_key(app_name: &AppName, user_id: &UserId, key: &str) -> String {
    format!("{}:{}:{key}", app_name.as_str(), user_id.as_str())
}
