use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;

use super::{AuthResult, ISSUER, failure};

#[derive(Deserialize)]
pub(super) struct Identity {
    pub sub: String,
    pub email: Option<String>,
    nonce: Option<String>,
    iat: u64,
    aud: serde_json::Value,
    azp: Option<String>,
}

pub(super) fn verify(
    token: &str,
    client: &str,
    nonce: Option<&str>,
    keys: &JwkSet,
) -> AuthResult<Identity> {
    let header = decode_header(token).map_err(|_| failure("invalid ID token header"))?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err(failure("unsupported ID token signing algorithm"));
    }
    let key = header
        .kid
        .as_ref()
        .and_then(|id| keys.find(id))
        .ok_or_else(|| failure("ID token signing key is unavailable"))?;
    let key = DecodingKey::from_jwk(key).map_err(|_| failure("invalid issuer signing key"))?;
    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[client]);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["sub", "exp", "aud", "iss"]);
    validation.validate_nbf = true;
    validation.leeway = 5;
    let identity = decode::<Identity>(token, &key, &validation)
        .map_err(|_| failure("ID token signature or claims failed validation"))?
        .claims;
    if nonce.is_some_and(|nonce| identity.nonce.as_deref() != Some(nonce))
        || identity.sub.is_empty()
        || identity.iat > super::now()?.saturating_add(5)
    {
        return Err(failure(
            "ID token nonce, subject, or issued-at time is invalid",
        ));
    }
    if identity
        .aud
        .as_array()
        .is_some_and(|audience| audience.len() > 1)
        && identity.azp.as_deref() != Some(client)
    {
        return Err(failure(
            "ID token authorized party does not match this client",
        ));
    }
    Ok(identity)
}
