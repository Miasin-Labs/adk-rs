use adk_rs::{AppName, AuthCredential, CredentialService, FileCredentialService, UserId};

#[test]
fn file_credential_service_persists_and_redacts_debug_normal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("credentials.json");
    let app = AppName::new("app").unwrap();
    let user = UserId::new("user").unwrap();
    let service = FileCredentialService::new(path.clone());

    service
        .put_credential(
            &app,
            &user,
            "openai",
            AuthCredential::BearerToken("super-secret-token".to_owned()),
        )
        .unwrap();

    let reloaded = FileCredentialService::new(path);
    assert_eq!(
        reloaded.get_credential(&app, &user, "openai").unwrap(),
        Some(AuthCredential::BearerToken("super-secret-token".to_owned()))
    );
    let debug_text = format!(
        "{:?}",
        AuthCredential::BearerToken("super-secret-token".to_owned())
    );
    assert!(!debug_text.contains("super-secret-token"));
    assert!(debug_text.contains("<redacted"));
}
