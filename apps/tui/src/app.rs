use std::collections::VecDeque;
use std::fmt::Write as _;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use fishnet_core::protocol::{
    EventData, ModelReply, OperationRequest, OperationResult, ToolResult,
};
use fishnet_core::{Event, Limits, TaskState};
use fishnet_service::contract::{
    Approval, EventPage, Phase, Submission, TaskRequest, TaskSummary, TaskView,
};

pub const MAX_LINES: usize = 256;

#[derive(Clone, Debug)]
pub enum Action {
    Select(String),
    Submit(Submission),
    Approve { id: String, decision: Approval },
    Cancel(String),
    Refresh,
}
#[derive(Debug)]
pub enum Message {
    Tasks(Vec<TaskSummary>),
    Page(EventPage),
    Submitted(TaskView),
    Updated(TaskView),
    Connection { connected: bool, detail: String },
    Error(String),
}

#[derive(Clone, Debug)]
pub enum Modal {
    Code {
        fields: [String; 3],
        focus: usize,
    },
    Review {
        id: String,
        revision: u64,
        text: String,
        scroll: u16,
    },
    Cancel(String),
    Help,
    Notice {
        text: String,
        scroll: u16,
    },
}

#[derive(Debug)]
pub struct App {
    pub tasks: Vec<TaskSummary>,
    pub selected: Option<String>,
    pub snapshot: Option<TaskView>,
    pub lines: VecDeque<String>,
    pub connected: bool,
    pub notice: String,
    notices: VecDeque<String>,
    pub modal: Option<Modal>,
    pub scroll: u16,
    pub horizontal: u16,
    pub quit: bool,
    pub default_model: String,
    pub account: String,
}
impl Default for App {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            selected: None,
            snapshot: None,
            lines: VecDeque::new(),
            connected: false,
            notice: "Connecting. Start make daemon in another terminal if needed.".into(),
            notices: VecDeque::new(),
            modal: None,
            scroll: 0,
            horizontal: 0,
            quit: false,
            default_model: String::new(),
            account: "default".into(),
        }
    }
}
impl App {
    /// Apply one service observation. A late response cannot replace another task's view.
    pub fn update(&mut self, message: Message) -> Option<Action> {
        match message {
            Message::Tasks(tasks) => {
                self.tasks = tasks;
                if self.selected.is_none()
                    && let Some(first) = self.tasks.first()
                {
                    return Some(self.select(first.task_id.clone()));
                }
            }
            Message::Page(page) => {
                if self.selected.as_deref() != Some(&page.snapshot.task_id) {
                    return None;
                }
                if page.gap {
                    self.lines.clear();
                    self.set_notice("Event history gap. Snapshot recovered; loading retained events. Full journal is on disk.".into());
                }
                for event in page.events {
                    self.append(&event);
                }
                if let Some(message) = &page.snapshot.message
                    && self.snapshot.as_ref().and_then(|old| old.message.as_ref()) != Some(message)
                {
                    self.set_notice(safe(message, 1024));
                }
                self.snapshot = Some(page.snapshot);
            }
            Message::Submitted(view) => {
                self.notice = format!(
                    "Accepted request {}. A terminal check result determines success.",
                    view.request_id
                );
                let action = self.select(view.task_id.clone());
                self.snapshot = Some(view);
                return Some(action);
            }
            Message::Updated(view) => {
                if self.selected.as_deref() == Some(&view.task_id) {
                    self.snapshot = Some(view);
                }
                self.notice = "Command acknowledged. Task state comes from the service.".into();
            }
            Message::Connection { connected, detail } => {
                let changed = self.connected != connected;
                self.connected = connected;
                if changed || !connected {
                    self.set_notice(safe(&detail, 512));
                }
            }
            Message::Error(error) => self.set_notice(safe(&error, 1024)),
        }
        None
    }

    /// Convert a key to an explicit client command. It cannot grant capabilities locally.
    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }
        if self.modal.is_some() {
            return self.modal_key(key.code);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') => return self.navigate(1),
            KeyCode::Up | KeyCode::Char('k') => return self.navigate(-1),
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_add(10).min(
                    u16::try_from(self.lines.len())
                        .unwrap_or(u16::MAX)
                        .saturating_sub(1),
                );
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Left => self.horizontal = self.horizontal.saturating_sub(8),
            KeyCode::Right => self.horizontal = self.horizontal.saturating_add(8).min(512),
            KeyCode::End => {
                self.scroll = 0;
                self.horizontal = 0;
            }
            KeyCode::Char('r') => return Some(Action::Refresh),
            KeyCode::Char('n') => return self.fixture(),
            KeyCode::Char('p') => self.open_code(String::new()),
            KeyCode::Char('a') => self.open_review(),
            KeyCode::Char('c') => {
                if let Some(id) = &self.selected {
                    self.modal = Some(Modal::Cancel(id.clone()));
                }
            }
            KeyCode::Char('v') => {
                let mut text = self.notice.clone();
                for previous in self
                    .notices
                    .iter()
                    .rev()
                    .filter(|message| *message != &self.notice)
                {
                    text.push_str("\n\n");
                    text.push_str(previous);
                }
                self.modal = Some(Modal::Notice { text, scroll: 0 });
            }
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            _ => (),
        }
        None
    }

    pub fn open_code(&mut self, path: String) {
        self.modal = Some(Modal::Code {
            fields: [path, self.default_model.clone(), self.account.clone()],
            focus: 0,
        });
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(Modal::Code { fields, focus }) = &mut self.modal
            && let Some(field) = fields.get_mut(*focus)
        {
            for ch in text.chars().filter(|ch| !ch.is_control()) {
                if field.len() + ch.len_utf8() > 512 {
                    break;
                }
                field.push(ch);
            }
        }
    }

    /// Select a task and start its event cursor independently from another task.
    pub fn select(&mut self, id: String) -> Action {
        self.selected = Some(id.clone());
        self.snapshot = None;
        self.lines.clear();
        self.scroll = 0;
        self.horizontal = 0;
        Action::Select(id)
    }

    fn navigate(&mut self, direction: isize) -> Option<Action> {
        if self.tasks.is_empty() {
            return None;
        }
        let index = self
            .tasks
            .iter()
            .position(|task| Some(&task.task_id) == self.selected.as_ref())
            .unwrap_or(0);
        let next = index
            .saturating_add_signed(direction)
            .min(self.tasks.len().saturating_sub(1));
        self.tasks
            .get(next)
            .map(|task| task.task_id.clone())
            .map(|id| self.select(id))
    }

    fn fixture(&mut self) -> Option<Action> {
        match fishnet_service::new_id() {
            Ok(request_id) => Some(Action::Submit(Submission {
                request_id,
                task: TaskRequest::Fixture {
                    scenario: "success".into(),
                    delay_ms: 300,
                    limits: Limits {
                        deadline_ms: 10_000,
                        ..Limits::default()
                    },
                },
            })),
            Err(error) => {
                self.notice = error.to_string();
                None
            }
        }
    }

    fn open_review(&mut self) {
        if let Some(view) = &self.snapshot
            && let Some(grant) = &view.approval
        {
            let text = review(grant);
            self.modal = Some(Modal::Review {
                id: view.task_id.clone(),
                revision: grant.revision,
                text,
                scroll: 0,
            });
        } else {
            self.notice = "No pending approval for the selected task.".into();
        }
    }

    fn modal_key(&mut self, key: KeyCode) -> Option<Action> {
        let mut modal = self.modal.take()?;
        if key == KeyCode::Esc {
            return None;
        }
        let action = match &mut modal {
            Modal::Code { fields, focus } => {
                match key {
                    KeyCode::Tab => *focus = (*focus + 1) % 3,
                    KeyCode::BackTab => *focus = (*focus + 2) % 3,
                    KeyCode::Backspace => {
                        if let Some(field) = fields.get_mut(*focus) {
                            field.pop();
                        }
                    }
                    KeyCode::Char(ch) => {
                        if let Some(field) = fields.get_mut(*focus)
                            && !ch.is_control()
                            && field.len() + ch.len_utf8() <= 512
                        {
                            field.push(ch);
                        }
                    }
                    KeyCode::Enter => return self.submit_code(fields),
                    _ => (),
                }
                None
            }
            Modal::Review {
                id,
                revision,
                scroll,
                ..
            } => match key {
                KeyCode::Char('y') => Some(Action::Approve {
                    id: id.clone(),
                    decision: Approval {
                        expected_revision: *revision,
                        approve: true,
                    },
                }),
                KeyCode::Char('n') => Some(Action::Approve {
                    id: id.clone(),
                    decision: Approval {
                        expected_revision: *revision,
                        approve: false,
                    },
                }),
                KeyCode::Down | KeyCode::PageDown => {
                    *scroll = scroll.saturating_add(5);
                    None
                }
                KeyCode::Up | KeyCode::PageUp => {
                    *scroll = scroll.saturating_sub(5);
                    None
                }
                _ => None,
            },
            Modal::Cancel(id) => {
                if key == KeyCode::Char('y') {
                    Some(Action::Cancel(id.clone()))
                } else {
                    return None;
                }
            }
            Modal::Notice { scroll, .. } => {
                match key {
                    KeyCode::Down | KeyCode::PageDown => *scroll = scroll.saturating_add(5),
                    KeyCode::Up | KeyCode::PageUp => *scroll = scroll.saturating_sub(5),
                    KeyCode::Home => *scroll = 0,
                    _ => (),
                }
                None
            }
            Modal::Help => return None,
        };
        if action.is_none() {
            self.modal = Some(modal);
        }
        action
    }

    fn submit_code(&mut self, fields: &[String; 3]) -> Option<Action> {
        let path = fields.first().cloned().unwrap_or_default();
        let model = fields.get(1).filter(|s| !s.is_empty()).cloned();
        let account = fields.get(2).cloned().unwrap_or_default();
        if path.is_empty() || account.is_empty() {
            self.notice = "Task path and account are required.".into();
            self.modal = Some(Modal::Code {
                fields: fields.clone(),
                focus: 0,
            });
            return None;
        }
        match fishnet_service::new_id() {
            Ok(request_id) => Some(Action::Submit(Submission {
                request_id,
                task: TaskRequest::Code {
                    task_path: path,
                    model,
                    account,
                },
            })),
            Err(error) => {
                self.notice = error.to_string();
                None
            }
        }
    }

    fn set_notice(&mut self, notice: String) {
        self.notice.clone_from(&notice);
        if self.notices.back() != Some(&notice) {
            self.notices.push_back(notice);
        }
        if self.notices.len() > 16 {
            self.notices.pop_front();
        }
    }

    fn append(&mut self, event: &Event) {
        let prefix = format!("#{} turn {}", event.sequence, event.turn);
        let description = describe(&event.data);
        for line in format!("{prefix} {description}").lines().take(24) {
            self.lines.push_back(safe(line, 512));
            if self.lines.len() > MAX_LINES {
                self.lines.pop_front();
            }
        }
    }
}

/// Remove terminal control characters before presenting untrusted source or model text.
#[must_use]
pub fn safe(text: &str, limit: usize) -> String {
    fishnet_core::protocol::BoundedText::new(text, limit)
        .text
        .chars()
        .map(|ch| if terminal_control(ch) { ' ' } else { ch })
        .collect()
}

/// Describe verified, unverified, failed, cancelled, and unknown states with explicit labels.
#[must_use]
pub const fn status(phase: Phase, state: Option<TaskState>, incomplete: bool) -> &'static str {
    if phase.terminal()
        && (incomplete
            || matches!(
                state,
                Some(TaskState::Queued | TaskState::Running | TaskState::Cancelling)
            ))
    {
        return "UNKNOWN / INTERRUPTED";
    }
    match state {
        Some(TaskState::VerifiedSuccess) => "VERIFIED SUCCESS",
        Some(TaskState::UnverifiedCompletion) => "UNVERIFIED COMPLETION",
        Some(TaskState::Failed) => "FAILED",
        Some(TaskState::Cancelled) => "CANCELLED",
        Some(TaskState::Cancelling) => "CANCELLING",
        Some(TaskState::Running) => "RUNNING",
        Some(TaskState::Queued) => "QUEUED",
        None => match phase {
            Phase::AwaitingApproval => "AWAITING APPROVAL",
            Phase::Accepted => "ACCEPTED",
            Phase::Executing => "RUNNING",
            Phase::Rejected => "REJECTED",
            Phase::Cancelled => "CANCELLED",
            Phase::Finished | Phase::Interrupted => "UNKNOWN / INTERRUPTED",
        },
    }
}

fn describe(data: &EventData) -> String {
    match data {
        EventData::RunCreated { contract, .. } => {
            format!("Task started: {}", safe(&contract.objective, 1024))
        }
        EventData::StateChanged { to, .. } => format!("State: {to:?}"),
        EventData::OperationStarted { request } => match request {
            OperationRequest::Provider { .. } => "Model request".into(),
            OperationRequest::Tool { call, .. } => format!(
                "Tool {} {}",
                call.name,
                call.arguments
                    .get("path")
                    .or_else(|| call.arguments.get("command_id"))
                    .map_or(String::new(), ToString::to_string)
            ),
            OperationRequest::Check { checker, .. } => format!("Independent check: {checker}"),
        },
        EventData::OperationFinished { result } => match result {
            OperationResult::Model { reply, .. } => match reply {
                ModelReply::Final { text } => {
                    format!("Model completed (checks still required):\n{text}")
                }
                ModelReply::ToolCall(call) => format!("Model selected {}", call.name),
            },
            OperationResult::Tool { result } => match result {
                ToolResult::Found { key, value, .. } => format!("Result: {key} = {value}"),
                ToolResult::ExpectedNegative { key, .. } => {
                    format!("Expected missing result: {key}")
                }
                ToolResult::Rejected { error } => format!("Rejected: {error}"),
                ToolResult::Output { data } => tool_output(data),
            },
            OperationResult::Check { status, .. } => format!("Independent check: {status:?}"),
            OperationResult::Error { error, .. } => format!("Operation error: {error}"),
        },
        EventData::Incident {
            code, repeat_count, ..
        } => format!("Incident: {code}, repetitions {repeat_count}"),
        EventData::TaskFinished { outcome } => {
            format!("Outcome: {:?}: {}", outcome.state, outcome.reason)
        }
    }
}
fn tool_output(data: &serde_json::Value) -> String {
    match (data.get("output"), data.get("path")) {
        (Some(output), _) => format!(
            "Command exit {}\n{}\n{}",
            output.get("exit_code").unwrap_or(&serde_json::Value::Null),
            output
                .get("stdout")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
            output
                .get("stderr")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
        ),
        (None, Some(path)) => format!("File result: {path}"),
        (None, None) => "Tool result recorded".into(),
    }
}
fn quoted<T: serde::Serialize>(value: &T) -> String {
    let encoded = serde_json::to_string(value).unwrap_or_else(|_| "<unavailable>".into());
    let mut escaped = String::new();
    for ch in encoded.chars() {
        if terminal_control(ch) {
            let _ = write!(escaped, "\\u{:04x}", u32::from(ch));
        } else {
            escaped.push(ch);
        }
    }
    escaped
}
fn terminal_control(ch: char) -> bool {
    ch.is_control()
        || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{200e}'|'\u{200f}')
}

fn review(grant: &fishnet_service::contract::ApprovalView) -> String {
    let mut text = format!(
        "Provider: {}\nAccount: {}\nModel: {}\nRoot: {}\n\nObjective:\n{}\n\nWritable files:\n{}\nRequired changes:\n{}\nProtected files:\n{}\n\nIndependent checks: {}\nDeadline: {} ms; model turns: {}; output: {} bytes\n\nApproved commands:\n",
        safe(&grant.provider, 512),
        safe(&grant.account, 64),
        safe(
            grant.model.as_deref().unwrap_or("host/account default"),
            128
        ),
        quoted(&grant.root),
        safe(&grant.objective, 16_384),
        quoted(&grant.allowed_writes),
        quoted(&grant.required_changes),
        quoted(&grant.protected_paths),
        quoted(&grant.checks),
        grant.limits.deadline_ms,
        grant.limits.max_provider_calls,
        grant.limits.max_output_bytes
    );
    for command in &grant.commands {
        let _ = writeln!(
            text,
            "{}: {} {} ({} ms)",
            safe(&command.id, 64),
            quoted(&command.program),
            quoted(&command.arguments),
            command.timeout_ms
        );
    }
    let _ = write!(
        text,
        "\nGrant fingerprint: {}\n\nApproval permits source and command output to be sent to the configured provider.\nCommands use the host OS permissions.\ny approve | n reject | arrows/PageUp/PageDown scroll | Esc close",
        grant.fingerprint
    );
    text
}
