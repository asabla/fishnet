use std::path::Path;
use std::time::Duration;

use reqwest::{
    Method,
    header::{AUTHORIZATION, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::contract::{
    API_VERSION, ApiError, Approval, Envelope, EventPage, Result, Submission, TaskSummary, TaskView,
};

/// A thin authenticated client. It never creates a model or executes a tool.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: reqwest::Url,
    authorization: HeaderValue,
}
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}
impl Client {
    /// Ask the authenticated host to stop and cancel active work.
    ///
    /// # Errors
    /// Returns connection, authorization, or protocol errors. No command is retried.
    pub async fn shutdown(&self) -> Result<()> {
        self.post("v1/shutdown", ()).await
    }

    /// # Errors
    /// Rejects unsafe connection files, insecure endpoints, and HTTP initialization errors.
    pub fn connect(path: &Path) -> Result<Self> {
        let connection = crate::read_connection(path)?;
        let endpoint = reqwest::Url::parse(&connection.endpoint)
            .map_err(|_| ApiError::invalid("invalid service endpoint"))?;
        let local = endpoint
            .host_str()
            .is_some_and(|host| ["127.0.0.1", "localhost", "[::1]", "::1"].contains(&host));
        if (endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && local))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(ApiError::invalid(
                "service requires HTTPS or loopback HTTP without URL credentials",
            ));
        }
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", connection.token))
            .map_err(|_| ApiError::invalid("invalid service credential"))?;
        authorization.set_sensitive(true);
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| ApiError::invalid("HTTP client unavailable"))?;
        Ok(Self {
            http,
            endpoint,
            authorization,
        })
    }

    /// # Errors
    /// Returns connection, authorization, capacity, or service protocol errors.
    pub async fn list(&self) -> Result<Vec<TaskSummary>> {
        self.get("v1/tasks").await
    }
    /// # Errors
    /// Returns connection, authorization, missing task, or protocol errors.
    pub async fn snapshot(&self, id: &str) -> Result<TaskView> {
        safe_id(id)?;
        self.get(&format!("v1/tasks/{id}")).await
    }
    /// # Errors
    /// Returns authorization, validation, capacity, idempotency conflict, or transport errors.
    pub async fn submit(&self, request: Submission) -> Result<TaskView> {
        if matches!(request.task, crate::contract::TaskRequest::Chat { .. }) {
            self.require_conversation().await?;
        }
        self.post("v1/tasks", request).await
    }
    /// # Errors
    /// Returns authorization, stale approval, capacity, or transport errors.
    pub async fn approve(&self, id: &str, approval: Approval) -> Result<TaskView> {
        safe_id(id)?;
        self.post(&format!("v1/tasks/{id}/approval"), approval)
            .await
    }
    /// # Errors
    /// Returns authorization, missing task, terminal conflict, or transport errors.
    pub async fn cancel(&self, id: &str) -> Result<TaskView> {
        safe_id(id)?;
        self.post(&format!("v1/tasks/{id}/cancel"), ()).await
    }
    /// # Errors
    /// Returns authorization, cursor, capacity, or transport errors. No command is replayed.
    pub async fn events(&self, id: &str, after: u64, wait_ms: u64) -> Result<EventPage> {
        safe_id(id)?;
        self.get(&format!(
            "v1/tasks/{id}/events?after={after}&wait_ms={wait_ms}"
        ))
        .await
    }
    /// Check protocol support before a conversation can be submitted.
    ///
    /// # Errors
    /// Returns a local, not-sent error when the host needs an upgrade or is unavailable.
    pub async fn require_conversation(&self) -> Result<()> {
        let capabilities: Vec<String> = self.get("v1/capabilities").await.map_err(|error| {
                ApiError::new("not_sent", &format!("Conversation was not sent. Check the service version; finish active work, then run fishnet stop and reopen fishnet. {}", error.message))
            })?;
        if !capabilities.iter().any(|name| name == "conversation-v1") {
            return Err(ApiError::new(
                "not_sent",
                "This service does not support conversations. Finish active work, run fishnet stop, then reopen fishnet.",
            ));
        }
        Ok(())
    }
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request::<(), T>(Method::GET, path, None).await
    }
    async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: B) -> Result<T> {
        self.request(Method::POST, path, Some(body)).await
    }
    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<B>,
    ) -> Result<T> {
        let url = self
            .endpoint
            .join(path)
            .map_err(|_| ApiError::invalid("invalid service path"))?;
        let mut request = self
            .http
            .request(method, url)
            .header(AUTHORIZATION, self.authorization.clone());
        if let Some(body) = body {
            request = request.json(&Envelope::new(body));
        }
        let mut response = request.send().await.map_err(|_| {
            ApiError::new(
                "disconnected",
                "service request failed; reconnect and inspect before repeating a command",
            )
        })?;
        let success = response.status().is_success();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ApiError::new("disconnected", "service response interrupted"))?
        {
            if bytes.len().saturating_add(chunk.len()) > 1_048_576 {
                return Err(ApiError::invalid("service response exceeds 1 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let envelope: Envelope<serde_json::Value> = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::invalid("service returned invalid JSON"))?;
        if envelope.schema_version != API_VERSION {
            return Err(ApiError::invalid("unsupported service API version"));
        }
        if !success {
            return Err(serde_json::from_value(envelope.data)
                .unwrap_or_else(|_| ApiError::invalid("invalid service error")));
        }
        serde_json::from_value(envelope.data)
            .map_err(|_| ApiError::invalid("service response does not match its contract"))
    }
}
fn safe_id(id: &str) -> Result<()> {
    crate::contract::validate_key(id)
}
