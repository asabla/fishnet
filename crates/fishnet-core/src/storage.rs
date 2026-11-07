use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::protocol::{
    CheckStatus, Event, EventData, OperationRequest, OperationResult, Outcome, SCHEMA_VERSION,
    TaskState, ToolResult,
};

pub const MAX_EVENT_BYTES: usize = 65_536;
pub const MAX_EVENTS: usize = 256;
pub const MAX_RECORD_BYTES: u64 = (MAX_EVENT_BYTES * MAX_EVENTS) as u64;
static NEXT_RUN: AtomicU64 = AtomicU64::new(0);

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// # Errors
/// Rejects IDs that could escape the store or exceed the ID limit.
pub fn record_path(store: &Path, run_id: &str) -> io::Result<PathBuf> {
    if run_id.is_empty()
        || run_id.len() > 128
        || !run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(invalid(
            "run ID must contain only ASCII letters, digits, or hyphens (1..=128 bytes)",
        ));
    }
    Ok(store.join(format!("{run_id}.jsonl")))
}

pub(crate) struct Journal {
    file: File,
    pub run_id: String,
    sequence: u64,
}

impl Journal {
    pub(crate) fn create(store: &Path, reserved_id: Option<String>) -> io::Result<Self> {
        // Sync each new directory's parent, including a newly created store hierarchy.
        ensure_directory(store)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let run_id = reserved_id.unwrap_or_else(|| {
            format!(
                "{nanos}-{}-{}",
                std::process::id(),
                NEXT_RUN.fetch_add(1, Ordering::Relaxed)
            )
        });
        let path = record_path(store, &run_id)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        file.sync_all()?;
        sync_directory(store)?;
        Ok(Self {
            file,
            run_id,
            sequence: 0,
        })
    }

    pub(crate) async fn append_async(&mut self, event: &Event) -> io::Result<()> {
        let mut writer = Self {
            file: self.file.try_clone()?,
            run_id: self.run_id.clone(),
            sequence: self.sequence,
        };
        let event = event.clone();
        let sequence = event.sequence;
        tokio::task::spawn_blocking(move || writer.append(&event))
            .await
            .map_err(io::Error::other)??;
        self.sequence = sequence;
        Ok(())
    }

    pub(crate) fn append(&mut self, event: &Event) -> io::Result<()> {
        if event.sequence != self.sequence + 1 || event.sequence > MAX_EVENTS as u64 {
            return Err(invalid("journal event sequence or count exceeds limit"));
        }
        let mut bytes = serde_json::to_vec(event)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_EVENT_BYTES {
            return Err(invalid("journal event exceeds byte limit"));
        }
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.sequence = event.sequence;
        Ok(())
    }
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        ensure_directory(parent)?;
    }
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => (),
        Err(e) => return Err(e),
    }
    sync_directory(path)?;
    sync_directory(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Statistics {
    pub provider_attempts: u32,
    pub logical_tool_calls: u32,
    pub tool_results: u32,
    pub rejected_calls: u32,
    pub expected_negative_results: u32,
    pub incidents: u32,
    pub process_attempts: u32,
    pub provider_us: u64,
    pub tool_us: u64,
    pub check_us: u64,
    pub harness_us: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunRecord {
    pub schema_version: u32,
    pub run_id: String,
    pub state: TaskState,
    pub outcome: Option<Outcome>,
    /// Incomplete can mean a live run or a stopped process. Inspection does not guess.
    pub incomplete: bool,
    pub partial_tail: bool,
    pub pending_operation_ids: Vec<String>,
    pub statistics: Statistics,
    pub events: Vec<Event>,
}

/// Read a bounded snapshot. Never construct a provider or execute a tool here.
///
/// # Errors
/// Returns an error for I/O failures, corrupt records, or unsupported schema versions.
pub fn inspect(store: &Path, run_id: &str) -> io::Result<RunRecord> {
    let file = File::open(record_path(store, run_id)?)?;
    let size = file.metadata()?.len();
    if size > MAX_RECORD_BYTES {
        return Err(invalid("run record exceeds byte limit"));
    }
    let mut reader = BufReader::new(file.take(size));
    let mut replay = Replay::new(run_id);

    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take(MAX_EVENT_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if count > MAX_EVENT_BYTES {
            return Err(invalid("event exceeds byte limit"));
        }
        if line.last() != Some(&b'\n') {
            replay.record.partial_tail = true;
            break;
        }
        replay.apply(serde_json::from_slice(&line)?)?;
    }

    replay.record.pending_operation_ids = replay.pending.into_keys().collect();
    Ok(replay.record)
}

/// Rebuild local state from observations, without an execution capability.
struct Replay {
    record: RunRecord,
    pending: BTreeMap<String, (OperationRequest, u32)>,
    completed: BTreeMap<String, OperationResult>,
    last_elapsed: u64,
    contract: Option<crate::protocol::TaskContract>,
}

impl Replay {
    fn new(run_id: &str) -> Self {
        Self {
            record: RunRecord {
                schema_version: SCHEMA_VERSION,
                run_id: run_id.into(),
                state: TaskState::Queued,
                outcome: None,
                incomplete: true,
                partial_tail: false,
                pending_operation_ids: Vec::new(),
                statistics: Statistics::default(),
                events: Vec::new(),
            },
            pending: BTreeMap::new(),
            completed: BTreeMap::new(),
            last_elapsed: 0,
            contract: None,
        }
    }

    fn apply(&mut self, event: Event) -> io::Result<()> {
        self.validate_envelope(&event)?;
        if self.record.events.is_empty() && !matches!(event.data, EventData::RunCreated { .. }) {
            return Err(invalid("first event must create the run"));
        }
        match &event.data {
            EventData::RunCreated {
                contract, limits, ..
            } => {
                if !self.record.events.is_empty() {
                    return Err(invalid("duplicate run creation"));
                }
                limits.validate().map_err(invalid)?;
                if !contract.valid() {
                    return Err(invalid("unsupported task contract version"));
                }
                self.contract = Some(contract.clone());
                self.record.schema_version = event.schema_version;
            }
            EventData::StateChanged { from, to } => {
                if *from != self.record.state || !from.permits(*to) || to.is_terminal() {
                    return Err(invalid("invalid state transition"));
                }
                self.record.state = *to;
            }
            EventData::OperationStarted { request } => self.start_operation(&event, request)?,
            EventData::OperationFinished { result } => self.finish_operation(&event, result)?,
            EventData::Incident {
                first_operation_id, ..
            } => {
                if !self.completed.contains_key(first_operation_id) {
                    return Err(invalid("incident references an unknown operation"));
                }
                self.record.statistics.incidents += 1;
            }
            EventData::TaskFinished { outcome } => {
                self.finish_task(outcome, event.duration_us.unwrap_or(event.elapsed_us))?;
            }
        }
        self.last_elapsed = event.elapsed_us;
        self.record.events.push(event);
        Ok(())
    }

    fn validate_envelope(&self, event: &Event) -> io::Result<()> {
        if self.record.events.len() >= MAX_EVENTS
            || !(1..=SCHEMA_VERSION).contains(&event.schema_version)
            || (!self.record.events.is_empty()
                && event.schema_version != self.record.schema_version)
            || event.sequence != self.record.events.len() as u64 + 1
            || event.session_id != self.record.run_id
            || event.task_id != self.record.run_id
            || event.trace_id != self.record.run_id
            || event.elapsed_us < self.last_elapsed
        {
            return Err(invalid(
                "event schema, correlation, timing, or sequence mismatch",
            ));
        }
        if self.record.state.is_terminal() {
            return Err(invalid("event follows terminal outcome"));
        }
        if event
            .duration_us
            .is_some_and(|duration| duration > event.elapsed_us)
        {
            return Err(invalid("operation duration exceeds task elapsed time"));
        }
        Ok(())
    }

    fn start_operation(&mut self, event: &Event, request: &OperationRequest) -> io::Result<()> {
        let id = event
            .operation_id
            .as_ref()
            .ok_or_else(|| invalid("operation ID missing"))?;
        if self.record.state != TaskState::Running
            || event.attempt != Some(1)
            || self.pending.contains_key(id)
            || self.completed.contains_key(id)
            || !self.pending.is_empty()
        {
            return Err(invalid("invalid operation start"));
        }
        if event
            .parent_operation_id
            .as_ref()
            .is_some_and(|parent| !self.completed.contains_key(parent))
        {
            return Err(invalid("unknown parent operation"));
        }
        match request {
            OperationRequest::Provider {
                previous_tool_operation_id,
                ..
            } => {
                if previous_tool_operation_id.as_ref().is_some_and(|prior| {
                    !matches!(
                        self.completed.get(prior),
                        Some(OperationResult::Tool { .. })
                    )
                }) {
                    return Err(invalid("provider references an unknown tool result"));
                }
                self.record.statistics.provider_attempts += 1;
            }
            OperationRequest::Tool {
                effective_command, ..
            } => {
                self.record.statistics.logical_tool_calls += 1;
                self.record.statistics.process_attempts += u32::from(effective_command.is_some());
            }
            OperationRequest::Check {
                effective_command, ..
            } => {
                self.record.statistics.process_attempts += u32::from(effective_command.is_some());
            }
        }
        self.pending
            .insert(id.clone(), (request.clone(), event.turn));
        Ok(())
    }

    fn finish_operation(&mut self, event: &Event, result: &OperationResult) -> io::Result<()> {
        let id = event
            .operation_id
            .as_ref()
            .ok_or_else(|| invalid("operation ID missing"))?;
        let (request, turn) = self
            .pending
            .remove(id)
            .ok_or_else(|| invalid("operation finish without start"))?;
        if event.duration_us.is_none() || turn != event.turn || event.attempt != Some(1) {
            return Err(invalid("operation completion timing or turn missing"));
        }
        let matches_request = matches!(
            (&request, result),
            (_, OperationResult::Error { .. })
                | (
                    OperationRequest::Provider { .. },
                    OperationResult::Model { .. }
                )
                | (OperationRequest::Tool { .. }, OperationResult::Tool { .. })
                | (
                    OperationRequest::Check { .. },
                    OperationResult::Check { .. }
                )
        );
        if !matches_request {
            return Err(invalid("operation result type mismatch"));
        }
        self.validate_check(result)?;
        let duration = event.duration_us.unwrap_or_default();
        match request {
            OperationRequest::Provider { .. } => self.record.statistics.provider_us += duration,
            OperationRequest::Tool { .. } => self.record.statistics.tool_us += duration,
            OperationRequest::Check { .. } => self.record.statistics.check_us += duration,
        }
        if let OperationResult::Tool { result } = result {
            self.record.statistics.tool_results += 1;
            match result {
                ToolResult::Rejected { .. } => self.record.statistics.rejected_calls += 1,
                ToolResult::ExpectedNegative { .. } => {
                    self.record.statistics.expected_negative_results += 1;
                }
                ToolResult::Found { .. } | ToolResult::Output { .. } => (),
            }
        }
        self.completed.insert(id.clone(), result.clone());
        Ok(())
    }

    fn validate_check(&self, result: &OperationResult) -> io::Result<()> {
        let OperationResult::Check {
            status: CheckStatus::Passed,
            evidence_operation_id,
            workspace,
        } = result
        else {
            return Ok(());
        };
        if let Some(check) = workspace {
            let spec = self
                .contract
                .as_ref()
                .and_then(|contract| contract.workspace.as_ref())
                .ok_or_else(|| invalid("workspace check has no task contract"))?;
            if !spec.commands.contains(&check.command)
                || !spec.checks.contains(&check.command.id)
                || check.output.exit_code != Some(0)
                || !check.protected_unchanged
                || spec
                    .required_changes
                    .iter()
                    .any(|path| !check.changed_paths.contains(path))
            {
                return Err(invalid("workspace check contradicts its task contract"));
            }
        } else if !matches!(
            evidence_operation_id
                .as_ref()
                .and_then(|source| self.completed.get(source)),
            Some(OperationResult::Tool {
                result: ToolResult::Found { .. } | ToolResult::ExpectedNegative { .. }
            })
        ) {
            return Err(invalid(
                "passed check references no independent tool evidence",
            ));
        }
        Ok(())
    }

    fn finish_task(&mut self, outcome: &Outcome, total_us: u64) -> io::Result<()> {
        if !outcome.state.is_terminal()
            || !self.record.state.permits(outcome.state)
            || !self.pending.is_empty()
        {
            return Err(invalid("invalid terminal outcome or unfinished operation"));
        }
        if outcome.state == TaskState::VerifiedSuccess
            && !matches!(
                outcome
                    .evidence_operation_id
                    .as_ref()
                    .and_then(|id| self.completed.get(id)),
                Some(OperationResult::Check {
                    status: CheckStatus::Passed,
                    ..
                })
            )
        {
            return Err(invalid(
                "verified success requires a passed independent check",
            ));
        }
        if outcome.state == TaskState::VerifiedSuccess
            && let Some(spec) = self
                .contract
                .as_ref()
                .and_then(|contract| contract.workspace.as_ref())
        {
            for id in &spec.checks {
                if !self.completed.values().any(|result| matches!(result, OperationResult::Check {
                    status: CheckStatus::Passed, workspace: Some(check), .. } if &check.command.id == id)) {
                    return Err(invalid("verified outcome omits a required workspace check"));
                }
            }
        }
        let measured = self.record.statistics.provider_us
            + self.record.statistics.tool_us
            + self.record.statistics.check_us;
        self.record.statistics.harness_us =
            (self.record.schema_version >= 2).then(|| total_us.saturating_sub(measured));
        self.record.state = outcome.state;
        self.record.outcome = Some(outcome.clone());
        self.record.incomplete = false;
        Ok(())
    }
}
