use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::json;

use super::*;

type TestResult = Result<(), Box<dyn Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self(std::env::temp_dir().canonicalize()?.join(format!(
            "fishnet-auth-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn account() -> store::Account {
    store::Account {
        schema_version: 1,
        client_id: "oaiapp_test".into(),
        subject: "test-subject".into(),
        email: Some("fixture@example.invalid".into()),
        host_id: "urn:uuid:00000000-0000-4000-8000-000000000000".into(),
        refresh_uncertain: false,
        tokens: Some(store::Tokens {
            access_token: "test-access-not-a-real-token".into(),
            refresh_token: "test-refresh-not-a-real-token".into(),
            id_token: "test-id-not-a-real-token".into(),
            expires_at: u64::MAX,
            earliest_refresh_at: 0,
            scopes: vec!["chatgpt.tokens.use.direct".into()],
        }),
    }
}

fn discovery() -> Discovery {
    Discovery {
        issuer: ISSUER.into(),
        authorization_endpoint: format!("{ISSUER}/api/accounts/authorize"),
        token_endpoint: format!("{ISSUER}/api/accounts/oauth/token"),
        jwks_uri: format!("{ISSUER}/.well-known/jwks.json"),
        revocation_endpoint: format!("{ISSUER}/revoke"),
    }
}

#[test]
fn fresh_pkce_and_issued_client_reauthorization_keep_tokens_out_of_urls() -> TestResult {
    let attempt = SignInAttempt {
        redirect: "http://127.0.0.1:1455/auth/callback".into(),
        state: random_value()?,
        nonce: random_value()?,
        verifier: random_value()?,
    };
    assert_eq!(attempt.verifier.len(), 43);
    assert_ne!(attempt.state, attempt.nonce);
    let url = authorization_url(&discovery(), "host", None, &attempt)?;
    let values = url
        .query_pairs()
        .into_owned()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        values.get("client_id").map(String::as_str),
        Some("dynamic_agent_client")
    );
    assert_eq!(
        values.get("agent_name_hint").map(String::as_str),
        Some("fishnet")
    );
    assert_eq!(
        values.get("code_challenge"),
        Some(&URL_SAFE_NO_PAD.encode(Sha256::digest(attempt.verifier.as_bytes())))
    );
    let existing = account();
    let url = authorization_url(&discovery(), "host", Some(&existing), &attempt)?;
    assert!(!url.as_str().contains("test-id-not-a-real-token"));
    assert!(
        !url.query_pairs()
            .any(|(key, _)| key == "agent_name_hint" || key == "id_token_hint")
    );
    Ok(())
}

#[test]
fn callback_validates_state_client_identity_and_unique_parameters() -> TestResult {
    let callback = callback::parse(
        "/auth/callback?state=good&code=one-time-code&client_id=oaiapp_test",
        "good",
        None,
    )?;
    assert_eq!(callback.client_id, "oaiapp_test");
    for target in [
        "/auth/callback?state=bad&code=code&client_id=oaiapp_test",
        "/auth/callback?state=good&code=code",
        "/auth/callback?state=good&code=code&client_id=dynamic_agent_client",
        "/auth/callback?state=good&code=code&state=good&client_id=oaiapp_test",
        "/auth/callback?state=good&error=access_denied&client_id=oaiapp_test",
    ] {
        assert!(callback::parse(target, "good", None).is_err());
    }
    assert!(
        callback::parse(
            "/auth/callback?state=good&code=code&client_id=oaiapp_other",
            "good",
            Some("oaiapp_test")
        )
        .is_err()
    );
    assert_eq!(
        callback::parse(
            "/auth/callback?state=good&code=code",
            "good",
            Some("oaiapp_test")
        )?
        .client_id,
        "oaiapp_test"
    );
    Ok(())
}

fn signed_token(claims: &Value) -> Result<String, Box<dyn Error>> {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("fishnet-test-only".into());
    Ok(jsonwebtoken::encode(
        &header,
        claims,
        &EncodingKey::from_rsa_der(include_bytes!(
            "../../tests/fixtures/oauth-test-private.der"
        )),
    )?)
}

#[test]
fn id_tokens_require_signatures_issuer_audience_expiry_nonce_and_subject() -> TestResult {
    let keys = serde_json::from_str(include_str!("../../tests/fixtures/oauth-test-jwks.json"))?;
    let claims = json!({"iss":ISSUER,"aud":"oaiapp_test","sub":"fixture-subject","iat":now()?,"exp":now()?+300,"nonce":"nonce"});
    let valid = signed_token(&claims)?;
    assert_eq!(
        identity::verify(&valid, "oaiapp_test", Some("nonce"), &keys)?.sub,
        "fixture-subject"
    );
    assert!(identity::verify(&valid, "oaiapp_other", Some("nonce"), &keys).is_err());
    assert!(identity::verify(&valid, "oaiapp_test", Some("other"), &keys).is_err());
    for (field, value) in [
        ("iss", json!("https://attacker.invalid")),
        ("sub", json!("")),
        ("exp", json!(1)),
        ("iat", json!(now()? + 100)),
    ] {
        let mut altered = claims.clone();
        altered
            .as_object_mut()
            .ok_or("claims not an object")?
            .insert(field.into(), value);
        assert!(
            identity::verify(
                &signed_token(&altered)?,
                "oaiapp_test",
                Some("nonce"),
                &keys
            )
            .is_err()
        );
    }
    let mut damaged = valid.into_bytes();
    if let Some(byte) = damaged.last_mut() {
        *byte = if *byte == b'A' { b'B' } else { b'A' };
    }
    assert!(
        identity::verify(
            std::str::from_utf8(&damaged)?,
            "oaiapp_test",
            Some("nonce"),
            &keys
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn private_store_is_atomic_locked_and_keeps_host_and_account_identity() -> TestResult {
    let directory = Directory::new()?;
    let store = store::Store::open(&directory.0)?;
    let host = store.host_id()?;
    assert_eq!(host.len(), 45);
    store.save("first", &account())?;
    assert!(store::Store::open(&directory.0).is_err());
    assert!(store.load("../escape").is_err());
    drop(store);
    let store = store::Store::open(&directory.0)?;
    assert_eq!(store.host_id()?, host);
    assert_eq!(
        store.load("first")?.ok_or("missing account")?.subject,
        "test-subject"
    );
    assert!(store.load("other")?.is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory.0.join("account-first.json"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    Ok(())
}

#[tokio::test]
async fn saved_access_needs_plan_permission_and_uncertain_refresh_requires_sign_in() -> TestResult {
    let directory = Directory::new()?;
    let mut saved = account();
    {
        let store = store::Store::open(&directory.0)?;
        store.save("default", &saved)?;
    }
    let control = Cancellation::default();
    assert_eq!(
        bearer(&directory.0, "default", &control).await?,
        "test-access-not-a-real-token"
    );
    saved.refresh_uncertain = true;
    store::Store::open(&directory.0)?.save("default", &saved)?;
    assert!(bearer(&directory.0, "default", &control).await.is_err());
    saved.refresh_uncertain = false;
    saved.tokens.as_mut().ok_or("no token")?.scopes.clear();
    store::Store::open(&directory.0)?.save("default", &saved)?;
    assert!(bearer(&directory.0, "default", &control).await.is_err());
    let public_status = serde_json::to_string(&status(&directory.0, "default")?)?;
    assert!(
        !public_status.contains("test-access")
            && !public_status.contains("test-refresh")
            && !public_status.contains("test-id")
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn loose_permissions_and_symlinked_credentials_are_rejected() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new()?;
    let store = store::Store::open(&directory.0)?;
    store.save("default", &account())?;
    let path = directory.0.join("account-default.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert!(store.load("default").is_err());
    fs::remove_file(&path)?;
    std::os::unix::fs::symlink("host.json", &path)?;
    assert!(store.load("default").is_err());
    Ok(())
}
