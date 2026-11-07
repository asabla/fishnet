use std::collections::BTreeMap;
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::{AuthResult, failure};
use crate::Cancellation;

pub(super) struct Callback {
    pub code: String,
    pub client_id: String,
}

pub(super) fn parse(target: &str, state: &str, client: Option<&str>) -> AuthResult<Callback> {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| failure("invalid callback URL"))?;
    if url.path() != "/auth/callback" {
        return Err(failure("unexpected callback path"));
    }
    let mut values = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if values
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(failure("callback has duplicate parameters"));
        }
    }
    if values.get("state").map(String::as_str) != Some(state) {
        return Err(failure("callback state did not match the pending sign-in"));
    }
    if values.contains_key("error") {
        return Err(failure(
            "ChatGPT sign-in or plan permission was not granted",
        ));
    }
    let returned = values
        .get("client_id")
        .map(String::as_str)
        .or(client)
        .ok_or_else(|| failure("registration callback has no issued client ID"))?;
    if !returned.starts_with("oaiapp_")
        || returned.len() > 128
        || client.is_some_and(|selected| selected != returned)
    {
        return Err(failure(
            "callback client ID does not match this registration",
        ));
    }
    let code = values
        .get("code")
        .filter(|code| !code.is_empty() && code.len() <= 4_096)
        .ok_or_else(|| failure("callback authorization code is missing"))?;
    Ok(Callback {
        code: code.clone(),
        client_id: returned.into(),
    })
}

pub(super) async fn receive(
    listener: &TcpListener,
    state: &str,
    client: Option<&str>,
    cancellation: &Cancellation,
    deadline: Instant,
) -> AuthResult<(Callback, TcpStream)> {
    for _ in 0..32 {
        let (mut stream, peer) = cancellation
            .interrupt(deadline, listener.accept())
            .await?
            .map_err(|_| failure("callback listener failed"))?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let request_deadline = deadline.min(Instant::now() + std::time::Duration::from_secs(5));
        let request = match cancellation
            .interrupt(request_deadline, read_request(&mut stream))
            .await
        {
            Ok(Ok(request)) => request,
            _ if cancellation.is_cancelled() || Instant::now() >= deadline => {
                return Err(failure("sign-in cancelled or expired"));
            }
            _ => {
                respond(&mut stream, false, "Incomplete sign-in callback.").await;
                continue;
            }
        };
        let mut words = request
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace();
        let method = words.next().unwrap_or_default();
        let target = words.next().unwrap_or_default();
        if method != "GET" || !target.starts_with("/auth/callback?") {
            respond(&mut stream, false, "This is fishnet's sign-in callback.").await;
            continue;
        }
        match parse(target, state, client) {
            Ok(callback) => return Ok((callback, stream)),
            Err(error) => {
                respond(&mut stream, false, "Sign-in failed. Return to fishnet.").await;
                return Err(error);
            }
        }
    }
    Err(failure("sign-in callback request limit exceeded"))
}

pub(super) async fn respond(stream: &mut TcpStream, success: bool, text: &str) {
    let status = if success { "200 OK" } else { "400 Bad Request" };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.write_all(response.as_bytes()),
    )
    .await;
}

async fn read_request(stream: &mut TcpStream) -> AuthResult<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1_024];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|_| failure("cannot read sign-in callback"))?;
        bytes.extend_from_slice(buffer.get(..count).unwrap_or_default());
        if bytes.len() > 16_384 || count == 0 {
            return Err(failure("callback request is incomplete or oversized"));
        }
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(bytes).map_err(|_| failure("callback request is not UTF-8"))
}
