use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;
pub const POLICY_VERSION: &str = "fixture-read-only-v1";
pub const FIXTURE_REVISION: &str = "fixture-catalog-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    Cancelling,
    VerifiedSuccess,
    UnverifiedCompletion,
    Failed,
    Cancelled,
}

impl TaskState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::VerifiedSuccess | Self::UnverifiedCompletion | Self::Failed | Self::Cancelled
        )
    }

    #[must_use]
    pub const fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running | Self::Cancelling)
                | (
                    Self::Running,
                    Self::Cancelling
                        | Self::VerifiedSuccess
                        | Self::UnverifiedCompletion
                        | Self::Failed
                )
                | (Self::Cancelling, Self::Cancelled)
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Limits {
    pub deadline_ms: u64,
    pub max_provider_calls: u32,
    pub max_output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            deadline_ms: 5_000,
            max_provider_calls: 16,
            max_output_bytes: 4_096,
        }
    }
}

impl Limits {
    /// # Errors
    /// Returns an error when a limit is outside the supported M1 range.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.deadline_ms > 300_000
            || !(1..=32).contains(&self.max_provider_calls)
            || !(32..=8_192).contains(&self.max_output_bytes)
        {
            return Err(
                "limits require deadline_ms <= 300000, provider calls 1..=32, output bytes 32..=8192",
            );
        }
        Ok(())
    }
}

/// The check is selected by the harness, never by the model's final response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskContract {
    pub objective: String,
    pub expected_key: String,
    pub expected_value: Option<String>,
    pub policy_version: String,
    pub fixture_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelReply {
    ToolCall(ToolCall),
    Final { text: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InvalidArguments,
    PermissionDenied,
    ProviderFailure,
    OutputLimit,
    CallLimit,
    DeadlineExceeded,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolResult {
    Found {
        key: String,
        value: String,
        revision: String,
    },
    ExpectedNegative {
        key: String,
        revision: String,
    },
    Rejected {
        error: OperationError,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationRequest {
    Provider {
        provider: String,
        previous_tool_operation_id: Option<String>,
    },
    Tool {
        call: ToolCall,
        relevant_state: String,
    },
    Check {
        checker: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationResult {
    Model {
        reply: ModelReply,
    },
    Tool {
        result: ToolResult,
    },
    Check {
        status: CheckStatus,
        evidence_operation_id: Option<String>,
    },
    Error {
        error: OperationError,
        preview: Option<BoundedText>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoundedText {
    pub text: String,
    pub original_bytes: usize,
    pub truncated: bool,
}

impl BoundedText {
    #[must_use]
    pub fn new(text: &str, limit: usize) -> Self {
        let mut end = text.len().min(limit);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            text: text.get(..end).unwrap_or_default().into(),
            original_bytes: text.len(),
            truncated: end < text.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Outcome {
    pub state: TaskState,
    pub reason: String,
    pub evidence_operation_id: Option<String>,
    pub final_text: Option<BoundedText>,
}

/// Durable observations. Incidents have their own typed event; no memory is generated.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventData {
    RunCreated {
        contract: TaskContract,
        limits: Limits,
        provider_version: String,
    },
    StateChanged {
        from: TaskState,
        to: TaskState,
    },
    OperationStarted {
        request: OperationRequest,
    },
    OperationFinished {
        result: OperationResult,
    },
    Incident {
        code: String,
        first_operation_id: String,
        repeat_count: u32,
        relevant_state: String,
    },
    TaskFinished {
        outcome: Outcome,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub schema_version: u32,
    pub session_id: String,
    pub task_id: String,
    pub trace_id: String,
    pub sequence: u64,
    pub timestamp_unix_ms: u64,
    pub elapsed_us: u64,
    pub turn: u32,
    pub operation_id: Option<String>,
    pub parent_operation_id: Option<String>,
    pub attempt: Option<u32>,
    pub duration_us: Option<u64>,
    #[serde(flatten)]
    pub data: EventData,
}
