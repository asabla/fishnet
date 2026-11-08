use super::*;
use reqwest::StatusCode;
use serde_json::json;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn missing_legacy_routes_are_unsupported_instead_of_invalid_json() -> TestResult {
    for bytes in [
        b"".as_slice(),
        b"<html>Not Found</html>".as_slice(),
        br#"{"schema_version":1,"data":{"code":"not_found","message":"unknown route"}}"#.as_slice(),
    ] {
        let result = decode_response::<Vec<String>>(StatusCode::NOT_FOUND, bytes);
        assert!(!conversation_support(result)?);
    }
    Ok(())
}

#[test]
fn invalid_or_unauthorized_responses_cannot_be_treated_as_legacy_support() -> TestResult {
    let body = serde_json::to_vec(&Envelope::new(ApiError::new("unauthorized", "denied")))?;
    let result = conversation_support(decode_response(StatusCode::UNAUTHORIZED, &body));
    assert!(matches!(result,Err(error) if error.code=="unauthorized"));
    for (status, body) in [
        (StatusCode::OK, b"invalid".as_slice()),
        (StatusCode::INTERNAL_SERVER_ERROR, b"".as_slice()),
        (
            StatusCode::OK,
            br#"{"schema_version":2,"data":[]}"#.as_slice(),
        ),
        (
            StatusCode::OK,
            br#"{"schema_version":1,"data":"wrong shape"}"#.as_slice(),
        ),
    ] {
        assert!(conversation_support(decode_response(status, body)).is_err());
    }
    assert!(conversation_support(decode_response(
        StatusCode::OK,
        &serde_json::to_vec(&json!({"schema_version":1,"data":["conversation-v1"]}))?
    ))?);
    assert!(!conversation_support(decode_response(
        StatusCode::OK,
        &serde_json::to_vec(&Envelope::new(Vec::<String>::new()))?
    ))?);
    Ok(())
}
