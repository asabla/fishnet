//! `ChatGPT` OAuth account access. Credentials never enter a task or provider transcript.
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

use crate::Cancellation;
use crate::protocol::{ErrorKind, OperationError};

mod callback;
mod identity;
mod store;

pub type AuthResult<T> = Result<T, OperationError>;
const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

#[derive(Clone, Debug, Serialize)]
pub struct AccountStatus {
    pub label: String,
    pub connected: bool,
    pub plan_authorized: bool,
    pub email: Option<String>,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Model {
    pub id: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    revocation_endpoint: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: String,
    #[serde(default)]
    earliest_refresh_at: u64,
}

struct SignInAttempt {
    redirect: String,
    state: String,
    nonce: String,
    verifier: String,
}

struct AuthSession<'a> {
    client: &'a reqwest::Client,
    discovery: &'a Discovery,
    cancellation: &'a Cancellation,
    deadline: Instant,
}

/// Read non-secret connection status without refreshing or making a network request.
///
/// # Errors
/// Rejects unsafe credential paths, invalid records, or an account label outside the bounds.
pub fn status(directory: &Path, label: &str) -> AuthResult<AccountStatus> {
    let store = store::Store::open(directory)?;
    let account = store.load(label)?;
    Ok(account_status(label, account.as_ref()))
}

/// Start a fresh PKCE sign-in on a loopback callback. The caller opens the returned URL.
///
/// # Errors
/// Returns cancellation, callback, OAuth, identity-validation, permission, or storage failures.
pub async fn login(
    directory: &Path,
    label: &str,
    cancellation: &Cancellation,
    on_url: impl FnOnce(&str) + Send,
) -> AuthResult<AccountStatus> {
    let store = store::Store::open(directory)?;
    let previous = store.load(label)?;
    let host = store.host_id()?;
    let client = http_client()?;
    let deadline = Instant::now() + Duration::from_secs(600);
    let discovery = discover(&client, cancellation, deadline).await?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| failure("cannot bind loopback sign-in callback"))?;
    let redirect = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener
            .local_addr()
            .map_err(|_| failure("callback address unavailable"))?
            .port()
    );
    let attempt = SignInAttempt {
        redirect,
        state: random_value()?,
        nonce: random_value()?,
        verifier: random_value()?,
    };
    let url = authorization_url(&discovery, &host, previous.as_ref(), &attempt)?;
    on_url(url.as_str());
    let selected = previous.as_ref().map(|account| account.client_id.as_str());
    let (callback, mut stream) =
        callback::receive(&listener, &attempt.state, selected, cancellation, deadline).await?;
    let pending = previous.unwrap_or_else(|| store::Account {
        schema_version: 1,
        client_id: callback.client_id.clone(),
        subject: String::new(),
        email: None,
        host_id: host,
        tokens: None,
        refresh_uncertain: false,
    });
    // Keep an issued registration ID even if code exchange fails. Never activate unvalidated identity.
    store.save(label, &pending)?;
    let session = AuthSession {
        client: &client,
        discovery: &discovery,
        cancellation,
        deadline,
    };
    let result = finish_login(&session, pending, &callback, &attempt).await;
    match result {
        Ok(account) => {
            store.save(label, &account)?;
            callback::respond(
                &mut stream,
                true,
                "fishnet sign-in complete. You can return to the terminal.",
            )
            .await;
            Ok(account_status(label, Some(&account)))
        }
        Err(error) => {
            callback::respond(
                &mut stream,
                false,
                "fishnet sign-in failed. Return to the terminal for the result.",
            )
            .await;
            Err(error)
        }
    }
}

async fn finish_login(
    session: &AuthSession<'_>,
    mut account: store::Account,
    callback: &callback::Callback,
    attempt: &SignInAttempt,
) -> AuthResult<store::Account> {
    let AuthSession {
        client,
        discovery,
        cancellation,
        deadline,
    } = session;
    let form = [
        ("grant_type", "authorization_code"),
        ("client_id", callback.client_id.as_str()),
        ("code", callback.code.as_str()),
        ("code_verifier", attempt.verifier.as_str()),
        ("redirect_uri", attempt.redirect.as_str()),
        ("resource", RESOURCE),
    ];
    let tokens: TokenResponse = json_request(
        client.post(&discovery.token_endpoint).form(&form),
        cancellation,
        *deadline,
    )
    .await?;
    let keys = json_request(client.get(&discovery.jwks_uri), cancellation, *deadline).await?;
    let id_token = tokens
        .id_token
        .as_deref()
        .ok_or_else(|| failure("token exchange returned no ID token"))?;
    let identity = identity::verify(
        id_token,
        &account.client_id,
        Some(attempt.nonce.as_str()),
        &keys,
    )?;
    if !account.subject.is_empty() && identity.sub != account.subject {
        return Err(failure(
            "signed-in identity differs from the selected account; use a different account label",
        ));
    }
    account.subject = identity.sub;
    account.email = identity.email;
    account.tokens = Some(token_set(tokens, None)?);
    account.refresh_uncertain = false;
    Ok(account)
}

/// Return a bearer credential only to the provider adapter. Refreshes are serialized and atomic.
///
/// # Errors
/// Returns missing permission, uncertain renewal, token, network, or secure-storage errors.
pub async fn bearer(
    directory: &Path,
    label: &str,
    cancellation: &Cancellation,
) -> AuthResult<String> {
    let store = store::Store::open(directory)?;
    let mut account = store
        .load(label)?
        .ok_or_else(|| failure("run fishnet auth login first"))?;
    if account.refresh_uncertain {
        return Err(failure(
            "previous renewal outcome is unknown; sign in again before inference",
        ));
    }
    let tokens = account
        .tokens
        .as_ref()
        .ok_or_else(|| failure("account is signed out; run fishnet auth login"))?;
    if tokens.expires_at > now()?.saturating_add(360) {
        ensure_scope(&tokens.scopes)?;
        return Ok(tokens.access_token.clone());
    }
    if now()? < tokens.earliest_refresh_at {
        return Err(failure(
            "token renewal is not allowed yet; sign in again or wait",
        ));
    }
    let client = http_client()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let discovery = discover(&client, cancellation, deadline).await?;
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", account.client_id.as_str()),
        ("refresh_token", tokens.refresh_token.as_str()),
        ("resource", RESOURCE),
    ];
    account.refresh_uncertain = true;
    store.save(label, &account)?;
    let response: TokenResponse = json_request(
        client.post(&discovery.token_endpoint).form(&form),
        cancellation,
        deadline,
    )
    .await?;
    if let Some(id_token) = &response.id_token {
        let keys = json_request(client.get(&discovery.jwks_uri), cancellation, deadline).await?;
        let identity = identity::verify(id_token, &account.client_id, None, &keys)?;
        if identity.sub != account.subject {
            return Err(failure(
                "renewed ID token belongs to another account; sign in again",
            ));
        }
    }
    let replacement = token_set(
        response,
        account
            .tokens
            .as_ref()
            .map(|tokens| tokens.id_token.as_str()),
    )?;
    account.tokens = Some(replacement);
    account.refresh_uncertain = false;
    store.save(label, &account)?;
    account
        .tokens
        .map(|tokens| tokens.access_token)
        .ok_or_else(|| failure("renewal returned no usable token"))
}

/// List the selected account's visible models. No model entitlement is inferred locally.
///
/// # Errors
/// Returns credential, HTTP, or model-catalog errors.
pub async fn models(
    directory: &Path,
    label: &str,
    cancellation: &Cancellation,
) -> AuthResult<Vec<Model>> {
    let credential = bearer(directory, label, cancellation).await?;
    let value: Value = json_request(
        http_client()?
            .get(format!("{RESOURCE}/models"))
            .bearer_auth(credential),
        cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .await?;
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| failure("account model catalog has no models array"))?;
    Ok(models
        .iter()
        .filter(|model| model.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|model| {
            Some(Model {
                id: model.get("slug")?.as_str()?.into(),
                display_name: model.get("display_name")?.as_str()?.into(),
            })
        })
        .collect())
}

/// Revoke the renewable session and clear local tokens. Registration mapping remains available.
/// Returns whether remote revocation was confirmed.
///
/// # Errors
/// Returns secure-storage errors. Failed remote revocation is reported as `false`.
pub async fn logout(
    directory: &Path,
    label: &str,
    cancellation: &Cancellation,
) -> AuthResult<bool> {
    let store = store::Store::open(directory)?;
    let Some(mut account) = store.load(label)? else {
        return Ok(true);
    };
    let mut confirmed = account.tokens.is_none();
    if let Some(tokens) = &account.tokens {
        let client = http_client()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        if let Ok(discovery) = discover(&client, cancellation, deadline).await {
            let request = client.post(discovery.revocation_endpoint).form(&[
                ("token", tokens.refresh_token.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", account.client_id.as_str()),
            ]);
            if let Ok(Ok(response)) = cancellation.interrupt(deadline, request.send()).await {
                confirmed = response.status().as_u16() == 200;
            }
        }
    }
    account.tokens = None;
    account.refresh_uncertain = false;
    store.save(label, &account)?;
    Ok(confirmed)
}

fn authorization_url(
    discovery: &Discovery,
    host: &str,
    account: Option<&store::Account>,
    attempt: &SignInAttempt,
) -> AuthResult<reqwest::Url> {
    let mut url = reqwest::Url::parse(&discovery.authorization_endpoint)
        .map_err(|_| failure("invalid authorize endpoint"))?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(attempt.verifier.as_bytes()));
    let mut query = url.query_pairs_mut();
    query.extend_pairs([
        ("response_type", "code"),
        ("redirect_uri", attempt.redirect.as_str()),
        ("scope", SCOPE),
        ("resource", RESOURCE),
        ("state", attempt.state.as_str()),
        ("nonce", attempt.nonce.as_str()),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge.as_str()),
        ("ext_agent_host_id", host),
    ]);
    if let Some(account) = account {
        query.append_pair("client_id", &account.client_id);
        // Printed URLs omit ID-token hints. The issued client still selects the registration.
    } else {
        query
            .append_pair("client_id", "dynamic_agent_client")
            .append_pair("agent_name_hint", "fishnet");
    }
    drop(query);
    Ok(url)
}

fn token_set(response: TokenResponse, previous_id: Option<&str>) -> AuthResult<store::Tokens> {
    let scopes = response
        .scope
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure_scope(&scopes)?;
    if response.token_type != "Bearer"
        || response.access_token.is_empty()
        || response.refresh_token.is_empty()
        || response.access_token.len() > 32_768
        || response.refresh_token.len() > 32_768
        || response
            .id_token
            .as_ref()
            .is_some_and(|token| token.len() > 32_768)
        || !(1..=86_400).contains(&response.expires_in)
    {
        return Err(failure("invalid OAuth token response"));
    }
    Ok(store::Tokens {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        id_token: response
            .id_token
            .or_else(|| previous_id.map(str::to_owned))
            .ok_or_else(|| failure("ID token unavailable"))?,
        expires_at: now()?.saturating_add(response.expires_in),
        earliest_refresh_at: response.earliest_refresh_at,
        scopes,
    })
}

fn account_status(label: &str, account: Option<&store::Account>) -> AccountStatus {
    let tokens = account.and_then(|account| account.tokens.as_ref());
    AccountStatus {
        label: label.into(),
        connected: tokens.is_some(),
        plan_authorized: tokens.is_some_and(|tokens| ensure_scope(&tokens.scopes).is_ok()),
        email: account.and_then(|account| account.email.clone()),
        expires_at: tokens.map(|tokens| tokens.expires_at),
    }
}

async fn discover(
    client: &reqwest::Client,
    cancellation: &Cancellation,
    deadline: Instant,
) -> AuthResult<Discovery> {
    let discovery: Discovery = json_request(
        client.get(format!("{ISSUER}/.well-known/openid-configuration")),
        cancellation,
        deadline,
    )
    .await?;
    if discovery.issuer != ISSUER {
        return Err(failure("unexpected OpenID issuer"));
    }
    for endpoint in [
        &discovery.authorization_endpoint,
        &discovery.token_endpoint,
        &discovery.jwks_uri,
        &discovery.revocation_endpoint,
    ] {
        let url =
            reqwest::Url::parse(endpoint).map_err(|_| failure("invalid discovery endpoint"))?;
        if url.scheme() != "https"
            || url.host_str() != Some("auth.openai.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(failure("untrusted discovery endpoint"));
        }
    }
    Ok(discovery)
}

async fn json_request<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
    cancellation: &Cancellation,
    deadline: Instant,
) -> AuthResult<T> {
    let mut response = cancellation
        .interrupt(deadline, request.send())
        .await?
        .map_err(|_| failure("authentication transport failed; credentials were not logged"))?;
    if !response.status().is_success() {
        return Err(failure(&format!(
            "account service returned HTTP {}; sign-in or entitlement may be required",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = cancellation
        .interrupt(deadline, response.chunk())
        .await
        .map_err(|_| failure("authentication response cancelled or expired"))?
        .map_err(|_| failure("authentication response interrupted"))?
    {
        if bytes.len() + chunk.len() > 1_048_576 {
            return Err(failure("account response exceeds size bound"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| failure("invalid account service response"))
}

fn http_client() -> AuthResult<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| failure("cannot initialize authentication HTTP client"))
}

fn ensure_scope(scopes: &[String]) -> AuthResult<()> {
    if !scopes
        .iter()
        .any(|scope| scope == "chatgpt.tokens.use.direct")
    {
        return Err(failure("ChatGPT plan usage was not authorized"));
    }
    Ok(())
}

fn random_value() -> AuthResult<String> {
    let mut bytes = [0_u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| failure("OS random source is unavailable"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn now() -> AuthResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| failure("system time precedes Unix epoch"))
}
fn failure(message: &str) -> OperationError {
    OperationError {
        kind: ErrorKind::PermissionDenied,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
