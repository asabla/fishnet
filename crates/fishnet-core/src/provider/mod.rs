use std::collections::BTreeMap;
use std::future::Future;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::control::Cancellation;
use crate::fixture::ScriptedProvider;
use crate::protocol::{ModelReply, OperationError, TaskContract, ToolResult};

pub mod openai;
mod sse;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderObservation {
    pub adapter: String,
    pub model: String,
    #[serde(default)]
    pub transport: String,
    pub response_id: Option<String>,
    pub native_output: Vec<Value>,
    pub raw_usage: Option<Value>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub first_event_us: Option<u64>,
    pub request_us: u64,
    pub prompt_fingerprints: BTreeMap<String, String>,
    pub stop_reason: String,
}

#[derive(Debug)]
pub struct ProviderTurn {
    pub reply: ModelReply,
    pub observation: Option<ProviderObservation>,
}

#[derive(Debug)]
pub struct ProviderContext<'a> {
    pub contract: &'a TaskContract,
    pub instructions: &'a str,
    pub tools: &'a [Value],
    pub remaining_provider_calls: u32,
}

pub trait ModelProvider: Send {
    fn version(&self) -> &'static str;

    fn respond(
        &mut self,
        context: ProviderContext<'_>,
        previous: Option<&ToolResult>,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> impl Future<Output = Result<ProviderTurn, OperationError>> + Send;
}

impl ModelProvider for ScriptedProvider {
    fn version(&self) -> &'static str {
        crate::fixture::PROVIDER_VERSION
    }

    async fn respond(
        &mut self,
        _context: ProviderContext<'_>,
        previous: Option<&ToolResult>,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> Result<ProviderTurn, OperationError> {
        self.complete_async(previous, cancellation, deadline)
            .await
            .map(|reply| ProviderTurn {
                reply,
                observation: None,
            })
    }
}

#[must_use]
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
