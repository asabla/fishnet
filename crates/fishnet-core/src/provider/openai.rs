use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde_json::{Value, json};

use super::{ModelProvider, ProviderContext, ProviderObservation, ProviderTurn, fingerprint, sse};
use crate::control::Cancellation;
use crate::protocol::{ErrorKind, ModelReply, OperationError, ToolCall, ToolResult};

struct Received {
    response: Value,
    first_event_us: Option<u64>,
    request_us: u64,
    transport: &'static str,
}

/// Direct Responses API adapter. It does not use an external agent runtime.
pub struct OpenAiProvider {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    authorization: HeaderValue,
    model: String,
    history: Vec<Value>,
    pending_call: Option<String>,
    chatgpt: bool,
    seen_calls: std::collections::BTreeSet<String>,
    #[cfg(test)]
    mock: Option<MockTransport>,
}

impl std::fmt::Debug for OpenAiProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiProvider")
            .field("model", &self.model)
            .field("history_items", &self.history.len())
            .finish_non_exhaustive()
    }
}

impl OpenAiProvider {
    /// # Errors
    /// Rejects invalid endpoints, credentials, model names, or HTTP client configuration.
    pub fn new(endpoint: &str, model: String, api_key: &str) -> Result<Self, OperationError> {
        let endpoint =
            reqwest::Url::parse(endpoint).map_err(|_| failure("invalid provider endpoint"))?;
        let local = endpoint
            .host_str()
            .is_some_and(|host| ["localhost", "127.0.0.1", "[::1]", "::1"].contains(&host));
        if (endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && local))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || model.is_empty()
            || model.len() > 128
            || api_key.is_empty()
        {
            return Err(failure(
                "provider requires HTTPS or loopback HTTP, a model, and externally supplied credentials",
            ));
        }
        let mut authorization = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|_| failure("invalid provider credential"))?;
        authorization.set_sensitive(true);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(15))
            .user_agent("fishnet/0.1.0")
            .build()
            .map_err(|_| failure("could not initialize HTTP client"))?;
        Ok(Self {
            client,
            endpoint,
            authorization,
            model,
            history: Vec::new(),
            pending_call: None,
            chatgpt: false,
            seen_calls: std::collections::BTreeSet::new(),
            #[cfg(test)]
            mock: None,
        })
    }

    /// # Errors
    /// Rejects invalid OAuth credentials or HTTP initialization failures.
    pub fn chatgpt(model: String, credential: &str) -> Result<Self, OperationError> {
        let mut provider = Self::new("https://api.openai.com/v1/responses", model, credential)?;
        provider.chatgpt = true;
        Ok(provider)
    }

    fn request(
        &mut self,
        context: &ProviderContext<'_>,
        previous: Option<&ToolResult>,
    ) -> Result<Value, OperationError> {
        if self.history.is_empty() {
            if let Some(conversation) = &context.contract.conversation {
                self.history.extend(
                    conversation
                        .messages
                        .iter()
                        .map(|message| json!({"role":message.role,"content":message.text})),
                );
            } else {
                self.history.push(json!({"role":"user","content":serde_json::to_string(context.contract).map_err(|_| failure("cannot encode task contract"))?}));
            }
        }
        match (self.pending_call.take(), previous) {
            (Some(call_id), Some(result)) => self.history.push(json!({"type":"function_call_output","call_id":call_id,
                "output":serde_json::to_string(result).map_err(|_| failure("cannot encode tool result"))?})),
            (None, None) => (),
            _ => return Err(failure("provider/tool result correlation mismatch")),
        }
        if context.contract.conversation.is_none() {
            self.history.push(json!({"role":"user","content":format!(
            "Execution budget: {} model turns remain, including this turn. Reserve a final text response. Scoped AGENTS.md instructions are already in the instructions above. The host independently repeats all required checks after the final response.",
            context.remaining_provider_calls)}));
        }
        let mut request = json!({"model":self.model,"instructions":context.instructions,"input":self.history,
            "tools":context.tools,"tool_choice":if context.remaining_provider_calls == 1 { "none" } else { "auto" },"parallel_tool_calls":false,"store":false,"stream":true,
            "include":["reasoning.encrypted_content"],"max_output_tokens":4096});
        if self.chatgpt {
            let object = request
                .as_object_mut()
                .ok_or_else(|| failure("request is not an object"))?;
            object.remove("max_output_tokens");
            if context.contract.conversation.is_none() {
                object.insert("tools".into(),json!([{"type":"namespace","name":"fishnet",
                "description":"Tools granted by this fishnet task contract","tools":context.tools}]));
            }
        }
        if serde_json::to_vec(&request)
            .map_err(|_| failure("cannot encode provider request"))?
            .len()
            > 262_144
        {
            return Err(failure(
                "provider input exceeds 256 KiB bound; no context was silently discarded",
            ));
        }
        Ok(request)
    }

    async fn stream(
        &self,
        request: &Value,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> Result<Received, OperationError> {
        let started = Instant::now();
        let mut response = cancellation
            .interrupt(
                deadline,
                self.client
                    .post(self.endpoint.clone())
                    .header(AUTHORIZATION, self.authorization.clone())
                    .json(request)
                    .send(),
            )
            .await?
            .map_err(|_| failure("provider request transport failed; no automatic retry"))?;
        if !response.status().is_success() {
            return Err(failure(&format!(
                "provider HTTP status {}; response body omitted",
                response.status().as_u16()
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type.is_empty() && !content_type.starts_with("text/event-stream") {
            let safe_type: String = content_type
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "/;= -_.".contains(*c))
                .take(80)
                .collect();
            return Err(failure(&format!(
                "provider response is not SSE (content-type: {safe_type})"
            )));
        }
        self.receive_stream(&mut response, cancellation, deadline, started)
            .await
    }

    async fn receive_stream(
        &self,
        response: &mut reqwest::Response,
        cancellation: &Cancellation,
        deadline: Instant,
        started: Instant,
    ) -> Result<Received, OperationError> {
        let transport = if response
            .headers()
            .contains_key(reqwest::header::CONTENT_TYPE)
        {
            "sse"
        } else {
            "sse-untyped"
        };
        let mut decoder = sse::Decoder::default();
        let mut first_event = None;
        loop {
            let chunk = cancellation.interrupt(deadline,response.chunk()).await?
                .map_err(|_| failure("provider stream interrupted; no tool was dispatched from partial arguments"))?
                .ok_or_else(|| failure("provider stream ended without response.completed"))?;
            for event in decoder.push(&chunk)? {
                first_event.get_or_insert_with(|| micros(started.elapsed()));
                if let Some(response) = terminal_response(&event)? {
                    return Ok(Received {
                        response,
                        first_event_us: first_event,
                        request_us: micros(started.elapsed()),
                        transport,
                    });
                }
            }
        }
    }

    fn accept(&mut self, response: &Value) -> Result<(ModelReply, Vec<Value>), OperationError> {
        if response.get("status").and_then(Value::as_str) != Some("completed") {
            return Err(failure("provider response is not completed"));
        }
        let native = response
            .get("output")
            .and_then(Value::as_array)
            .ok_or_else(|| failure("provider response has no output array"))?
            .clone();
        if serde_json::to_vec(&native)
            .map_err(|_| failure("cannot encode native output"))?
            .len()
            > 32_768
        {
            return Err(failure(
                "native provider output exceeds 32768 byte persistence bound",
            ));
        }
        let mut calls = Vec::new();
        let mut text = String::new();
        for item in &native {
            match item.get("type").and_then(Value::as_str) {
                Some("function_call") => calls.push(parse_call(item)?),
                Some("message") => collect_text(item, &mut text)?,
                Some("reasoning") => (),
                _ => {
                    return Err(failure(
                        "unsupported native output item; no built-in tool was authorized",
                    ));
                }
            }
        }
        let reply = if calls.len() == 1 {
            let (call_id, call) = calls
                .pop()
                .ok_or_else(|| failure("missing function call"))?;
            if !self.seen_calls.insert(call_id.clone()) {
                return Err(failure(
                    "provider reused a completed function call ID; no tool was re-executed",
                ));
            }
            self.pending_call = Some(call_id);
            ModelReply::ToolCall(call)
        } else if calls.is_empty() && !text.is_empty() {
            ModelReply::Final { text }
        } else {
            return Err(failure("expected one complete function call or final text"));
        };
        self.history.extend(native.iter().cloned());
        Ok((reply, native))
    }
}

impl ModelProvider for OpenAiProvider {
    fn version(&self) -> &'static str {
        "openai-responses-v1"
    }

    async fn respond(
        &mut self,
        context: ProviderContext<'_>,
        previous: Option<&ToolResult>,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> Result<ProviderTurn, OperationError> {
        let request = self.request(&context, previous)?;
        let sections = ["instructions", "tools", "input", "model"]
            .into_iter()
            .map(|section| {
                let bytes = serde_json::to_vec(request.get(section).unwrap_or(&Value::Null))
                    .unwrap_or_default();
                (section.into(), fingerprint(&bytes))
            })
            .collect::<BTreeMap<_, _>>();
        #[cfg(test)]
        let response = if let Some(mock) = &mut self.mock {
            mock.respond(&request, cancellation, deadline)
        } else {
            self.stream(&request, cancellation, deadline).await
        };
        #[cfg(not(test))]
        let response = self.stream(&request, cancellation, deadline).await;
        let Received {
            response,
            first_event_us,
            request_us,
            transport,
        } = response?;
        let (reply, native_output) = self.accept(&response)?;
        let raw_usage = response
            .get("usage")
            .filter(|value| !value.is_null())
            .cloned();
        let usage = raw_usage.as_ref();
        let observation = ProviderObservation {
            adapter: self.version().into(),
            model: response
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(&self.model)
                .into(),
            transport: transport.into(),
            response_id: response
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            native_output,
            input_tokens: usage
                .and_then(|value| value.get("input_tokens"))
                .and_then(Value::as_u64),
            output_tokens: usage
                .and_then(|value| value.get("output_tokens"))
                .and_then(Value::as_u64),
            cached_input_tokens: usage
                .and_then(|value| value.pointer("/input_tokens_details/cached_tokens"))
                .and_then(Value::as_u64),
            raw_usage,
            first_event_us,
            request_us,
            prompt_fingerprints: sections,
            stop_reason: "completed".into(),
        };
        Ok(ProviderTurn {
            reply,
            observation: Some(observation),
        })
    }
}

fn parse_call(item: &Value) -> Result<(String, ToolCall), OperationError> {
    if item
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status != "completed")
    {
        return Err(failure("function call is not complete"));
    }
    if item
        .get("namespace")
        .and_then(Value::as_str)
        .is_some_and(|namespace| namespace != "fishnet")
    {
        return Err(failure("function call uses an ungranted namespace"));
    }
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .ok_or_else(|| failure("function call has no valid call ID"))?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| failure("function call has no name"))?;
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .ok_or_else(|| failure("function call has no argument string"))?;
    Ok((
        call_id.into(),
        ToolCall {
            name: name.into(),
            arguments: serde_json::from_str(arguments)
                .map_err(|_| failure("function call arguments contain malformed JSON"))?,
        },
    ))
}

fn collect_text(item: &Value, text: &mut String) -> Result<(), OperationError> {
    let content = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| failure("message content is not an array"))?;
    for part in content {
        match part.get("type").and_then(Value::as_str) {
            Some("output_text") => text.push_str(
                part.get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| failure("message text missing"))?,
            ),
            Some("refusal") => return Err(failure("model refused the task")),
            _ => return Err(failure("unsupported message content")),
        }
    }
    Ok(())
}

pub(super) fn failure(message: &str) -> OperationError {
    OperationError {
        kind: ErrorKind::ProviderFailure,
        message: message.into(),
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn terminal_response(event: &Value) -> Result<Option<Value>, OperationError> {
    match event.get("type").and_then(Value::as_str) {
        Some("response.completed") => event
            .get("response")
            .cloned()
            .map(Some)
            .ok_or_else(|| failure("completed event has no response")),
        Some("response.failed" | "response.incomplete" | "error") => {
            let code = event
                .pointer("/response/error/code")
                .or_else(|| event.pointer("/error/code"))
                .and_then(Value::as_str);
            let code = code
                .filter(|code| {
                    [
                        "subscription_sharing_usage_limit_exceeded",
                        "subscription_sharing_usage_unavailable",
                        "model_not_found",
                        "rate_limit_exceeded",
                        "insufficient_quota",
                        "context_length_exceeded",
                        "credit_balance_exhausted",
                    ]
                    .contains(code)
                })
                .unwrap_or("unspecified");
            Err(failure(&format!(
                "provider returned failed or incomplete output ({code})"
            )))
        }
        Some(_) => Ok(None),
        None => Err(failure("provider stream event has no type")),
    }
}

#[cfg(test)]
struct MockTransport {
    streams: std::collections::VecDeque<Vec<Vec<u8>>>,
    requests: Vec<Value>,
}

#[cfg(test)]
impl MockTransport {
    fn respond(
        &mut self,
        request: &Value,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> Result<Received, OperationError> {
        self.requests.push(request.clone());
        let chunks = self
            .streams
            .pop_front()
            .ok_or_else(|| failure("mock response queue exhausted"))?;
        let mut decoder = sse::Decoder::default();
        for chunk in chunks {
            cancellation.check(deadline)?;
            for event in decoder.push(&chunk)? {
                if let Some(response) = terminal_response(&event)? {
                    return Ok(Received {
                        response,
                        first_event_us: Some(0),
                        request_us: 0,
                        transport: "sse",
                    });
                }
            }
        }
        Err(failure("provider stream ended without response.completed"))
    }
}

#[cfg(test)]
mod tests;
