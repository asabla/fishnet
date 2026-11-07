use fishnet_core::storage::Statistics;
use fishnet_core::{Event, Limits, Outcome, TaskState};
use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;
pub const RETAINED_EVENTS: usize = 128;
pub const PAGE_EVENTS: usize = 8;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub schema_version: u32,
    pub data: T,
}
impl<T> Envelope<T> {
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self {
            schema_version: API_VERSION,
            data,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    pub request_id: String,
    pub task: TaskRequest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskRequest {
    Fixture {
        scenario: String,
        delay_ms: u64,
        limits: Limits,
    },
    Code {
        task_path: String,
        model: Option<String>,
        account: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    AwaitingApproval,
    Accepted,
    Executing,
    Finished,
    Interrupted,
    Rejected,
    Cancelled,
}
impl Phase {
    #[must_use]
    pub const fn terminal(self) -> bool {
        matches!(
            self,
            Self::Finished | Self::Interrupted | Self::Rejected | Self::Cancelled
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub expected_revision: u64,
    pub approve: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApprovalView {
    pub revision: u64,
    pub root: std::path::PathBuf,
    pub objective: String,
    pub allowed_writes: Vec<String>,
    pub commands: Vec<fishnet_core::process::CommandSpec>,
    pub checks: Vec<String>,
    pub fingerprint: String,
    pub provider: String,
    pub account: String,
    pub model: Option<String>,
    pub required_changes: Vec<String>,
    pub protected_paths: Vec<String>,
    pub limits: Limits,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunView {
    pub state: TaskState,
    pub outcome: Option<Outcome>,
    pub incomplete: bool,
    pub partial_tail: bool,
    pub pending_operation_ids: Vec<String>,
    pub statistics: Statistics,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub request_id: String,
    pub phase: Phase,
    pub state: Option<TaskState>,
    pub objective: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskView {
    pub task_id: String,
    pub request_id: String,
    pub phase: Phase,
    pub message: Option<String>,
    pub approval: Option<ApprovalView>,
    pub approval_decision: Option<bool>,
    pub approval_wait_ms: Option<u64>,
    pub run: Option<RunView>,
    pub cursor: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EventPage {
    pub snapshot: TaskView,
    pub events: Vec<Event>,
    pub cursor: u64,
    pub retained_from: u64,
    pub gap: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
impl ApiError {
    #[must_use]
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    #[must_use]
    pub fn invalid(message: &str) -> Self {
        Self::new("invalid_request", message)
    }
    #[must_use]
    pub fn storage() -> Self {
        Self::new(
            "storage_error",
            "durable service storage failed; inspect before retrying",
        )
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}
pub type Result<T> = std::result::Result<T, ApiError>;

/// Validate an opaque service command or task identity.
///
/// # Errors
/// Rejects identities that can escape storage or URL boundaries.
pub fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 64
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(ApiError::invalid(
            "request ID requires 1..=64 ASCII letters, digits, hyphens, or underscores",
        ));
    }
    Ok(())
}
