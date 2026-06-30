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
    #[error("credential store encryption error: {message}")]
    Crypto { message: String },
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

/// A file-backed credential store that encrypts the credential blob at rest
/// using ChaCha20-Poly1305 AEAD. The key is derived from a passphrase (e.g.
/// from an env var) via SHA-256, so the on-disk file is ciphertext, not the
/// plaintext JSON written by [`FileCredentialService`].
#[derive(Clone)]
pub struct EncryptedFileCredentialService {
    path: PathBuf,
    key: [u8; 32],
    lock: Arc<Mutex<()>>,
}

impl EncryptedFileCredentialService {
    /// Build a store at `path`, deriving the AEAD key from `passphrase`.
    pub fn new(path: impl Into<PathBuf>, passphrase: &str) -> Self {
        use sha2::{Digest, Sha256};
        let key = Sha256::digest(passphrase.as_bytes()).into();
        Self {
            path: path.into(),
            key,
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Build a store whose passphrase is read from the `ADK_CREDENTIAL_KEY`
    /// environment variable.
    pub fn from_env(path: impl Into<PathBuf>) -> Result<Self, AuthError> {
        let passphrase = std::env::var("ADK_CREDENTIAL_KEY").map_err(|_| AuthError::Crypto {
            message: "ADK_CREDENTIAL_KEY environment variable is not set".to_owned(),
        })?;
        Ok(Self::new(path, &passphrase))
    }

    fn read(&self) -> Result<BTreeMap<String, AuthCredential>, AuthError> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let blob = fs::read(&self.path).map_err(|source| AuthError::Io { source })?;
        let plaintext = decrypt(&self.key, &blob)?;
        serde_json::from_slice(&plaintext).map_err(|source| AuthError::Json { source })
    }

    fn write(&self, credentials: &BTreeMap<String, AuthCredential>) -> Result<(), AuthError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| AuthError::Io { source })?;
        }
        let plaintext =
            serde_json::to_vec(credentials).map_err(|source| AuthError::Json { source })?;
        let blob = encrypt(&self.key, &plaintext)?;
        fs::write(&self.path, blob).map_err(|source| AuthError::Io { source })
    }
}

impl std::fmt::Debug for EncryptedFileCredentialService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the key material.
        f.debug_struct("EncryptedFileCredentialService")
            .field("path", &self.path)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl CredentialService for EncryptedFileCredentialService {
    fn put_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
        credential: AuthCredential,
    ) -> Result<(), AuthError> {
        let _guard = self.lock.lock().map_err(|_| AuthError::Poisoned)?;
        let mut credentials = self.read()?;
        credentials.insert(credential_key(app_name, user_id, key), credential);
        self.write(&credentials)
    }

    fn get_credential(
        &self,
        app_name: &AppName,
        user_id: &UserId,
        key: &str,
    ) -> Result<Option<AuthCredential>, AuthError> {
        let _guard = self.lock.lock().map_err(|_| AuthError::Poisoned)?;
        Ok(self
            .read()?
            .get(&credential_key(app_name, user_id, key))
            .cloned())
    }
}

/// Encrypt `plaintext` as `nonce(12) || ciphertext+tag`.
fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, AuthError> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Nonce};

    let cipher = ChaCha20Poly1305::new(key.into());
    // A unique 96-bit nonce per write, derived from the system clock + a
    // counter is ideal; here we use the nanosecond clock which is unique enough
    // for sequential single-writer credential files.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&nanos.to_le_bytes()[..12]);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|error| AuthError::Crypto {
            message: error.to_string(),
        })?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a `nonce(12) || ciphertext+tag` blob.
fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, AuthError> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Nonce};

    if blob.len() < 12 {
        return Err(AuthError::Crypto {
            message: "ciphertext too short".to_owned(),
        });
    }
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let cipher = ChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|error| AuthError::Crypto {
            message: error.to_string(),
        })
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

#[cfg(test)]
mod encrypted_tests {
    use super::*;

    #[test]
    fn encrypted_credential_roundtrips_and_is_ciphertext_on_disk_normal() {
        let dir = std::env::temp_dir().join(format!("adk-enc-cred-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("creds.enc");
        let app = AppName::new("app").unwrap();
        let user = UserId::new("user").unwrap();
        let secret = "super-secret-token-value";

        let svc = EncryptedFileCredentialService::new(&path, "correct horse battery staple");
        svc.put_credential(&app, &user, "openai", AuthCredential::ApiKey(secret.to_owned()))
            .unwrap();

        // On-disk bytes must NOT contain the plaintext secret.
        let on_disk = std::fs::read(&path).unwrap();
        assert!(
            !on_disk.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "secret must not appear in plaintext on disk"
        );

        // A fresh instance with the same passphrase round-trips the value.
        let svc2 = EncryptedFileCredentialService::new(&path, "correct horse battery staple");
        let loaded = svc2.get_credential(&app, &user, "openai").unwrap();
        assert!(matches!(loaded, Some(AuthCredential::ApiKey(ref k)) if k == secret));

        // A wrong passphrase fails to decrypt.
        let wrong = EncryptedFileCredentialService::new(&path, "wrong passphrase");
        assert!(wrong.get_credential(&app, &user, "openai").is_err());

        // Debug output never leaks the key.
        let dbg = format!("{svc:?}");
        assert!(dbg.contains("<redacted>"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
