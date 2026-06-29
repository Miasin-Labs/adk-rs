use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fmt, fs};

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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl fmt::Debug for AuthCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiKey(secret) => formatter
                .debug_tuple("ApiKey")
                .field(&redacted_secret(secret))
                .finish(),
            Self::BearerToken(secret) => formatter
                .debug_tuple("BearerToken")
                .field(&redacted_secret(secret))
                .finish(),
            Self::OAuth2 {
                access_token,
                refresh_token,
                expires_at_epoch,
            } => formatter
                .debug_struct("OAuth2")
                .field("access_token", &redacted_secret(access_token))
                .field(
                    "refresh_token",
                    &refresh_token.as_deref().map(redacted_secret),
                )
                .field("expires_at_epoch", expires_at_epoch)
                .finish(),
            Self::ServiceAccountJson(secret) => formatter
                .debug_tuple("ServiceAccountJson")
                .field(&redacted_secret(secret))
                .finish(),
        }
    }
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
    #[error("credential store I/O failed")]
    Io { source: std::io::Error },
    #[error("credential store JSON failed")]
    Json { source: serde_json::Error },
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

#[derive(Debug, Clone)]
pub struct FileCredentialService {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl FileCredentialService {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Arc::new(Mutex::new(())),
        }
    }
}

impl CredentialService for FileCredentialService {
    fn put_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
        credential: AuthCredential,
    ) -> Result<(), AuthError> {
        let _guard = self.lock.lock().map_err(|_| AuthError::Poisoned)?;
        let mut credentials = read_credentials(&self.path)?;
        credentials.insert(credential_key(app_name, user_id, key), credential);
        write_credentials(&self.path, &credentials)
    }

    fn get_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
    ) -> Result<Option<AuthCredential>, AuthError> {
        let _guard = self.lock.lock().map_err(|_| AuthError::Poisoned)?;
        Ok(read_credentials(&self.path)?
            .get(&credential_key(app_name, user_id, key))
            .cloned())
    }
}

fn credential_key(app_name: &AppName, user_id: &UserId, key: &str) -> String {
    format!("{}:{}:{key}", app_name.as_str(), user_id.as_str())
}

fn read_credentials(path: &PathBuf) -> Result<BTreeMap<String, AuthCredential>, AuthError> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = fs::read(path).map_err(|source| AuthError::Io { source })?;
    serde_json::from_slice(&bytes).map_err(|source| AuthError::Json { source })
}

fn write_credentials(
    path: &PathBuf,
    credentials: &BTreeMap<String, AuthCredential>,
) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AuthError::Io { source })?;
    }
    let bytes =
        serde_json::to_vec_pretty(credentials).map_err(|source| AuthError::Json { source })?;
    fs::write(path, bytes).map_err(|source| AuthError::Io { source })
}

fn redacted_secret(secret: &str) -> String {
    format!("<redacted:{} chars>", secret.len())
}
