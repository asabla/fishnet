mod composer;
mod input;
mod transcript;
pub use composer::{COMMANDS, Composer};
use std::collections::VecDeque;
use std::fmt::Write as _;
pub use transcript::{Entry, EntryKind, MAX_ENTRIES};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use fishnet_core::{Event, Limits};
use fishnet_service::contract::{
    Approval, EventPage, Submission, TaskRequest, TaskSummary, TaskView,
};
use fishnet_service::display::{describe, review};
pub use fishnet_service::display::{safe, status};

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
    Connection {
        connected: bool,
        detail: String,
    },
    Error(String),
    CommandError {
        reference: String,
        error: String,
        uncertain: bool,
    },
}

#[derive(Clone, Debug)]
pub enum Modal {
    Scope {
        objective: String,
        files: String,
        cursor: usize,
    },
    Code {
        fields: [String; 3],
        focus: usize,
        cursor: usize,
    },
    Review {
        id: String,
        revision: u64,
        text: String,
        summary: String,
        expanded: bool,
        scroll: u16,
    },
    Cancel(String),
    Help {
        scroll: u16,
    },
    Notice {
        text: String,
        scroll: u16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeLevel {
    Info,
    Connection,
    Attention,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitIntent {
    Attached,
    WaitingForCommands,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    Chat,
    Code,
}

#[derive(Clone, Debug)]
pub struct App {
    pub mode: Mode,
    pub conversation: Option<String>,
    pub appearance: crate::appearance::Appearance,
    pub tasks: Vec<TaskSummary>,
    pub selected: Option<String>,
    pub snapshot: Option<TaskView>,
    pub lines: VecDeque<String>,
    pub transcript: VecDeque<Entry>,
    pub composer: Composer,
    pub connected: bool,
    pub notice: String,
    pub notice_level: NoticeLevel,
    notices: VecDeque<String>,
    pub modal: Option<Modal>,
    pub scroll: u16,
    pub show_tasks: bool,
    pub quit: bool,
    pub pending: VecDeque<String>,
    pub exit_intent: ExitIntent,
    pub uncertain: VecDeque<String>,
    pub default_model: String,
    pub account: String,
    pub project: String,
    last_sequence: Option<u64>,
    submitted_prompt: Option<(String, String)>,
}
impl Default for App {
    fn default() -> Self {
        Self {
            mode: Mode::Chat,
            conversation: None,
            appearance: crate::appearance::Appearance::Color,
            tasks: Vec::new(),
            selected: None,
            snapshot: None,
            lines: VecDeque::new(),
            transcript: VecDeque::new(),
            composer: Composer::default(),
            connected: false,
            notice: "Connecting to the local service.".into(),
            notice_level: NoticeLevel::Info,
            notices: VecDeque::new(),
            modal: None,
            scroll: 0,
            show_tasks: false,
            quit: false,
            pending: VecDeque::new(),
            exit_intent: ExitIntent::Attached,
            uncertain: VecDeque::new(),
            default_model: String::new(),
            account: "default".into(),
            project: "workspace".into(),
            last_sequence: None,
            submitted_prompt: None,
        }
    }
}
impl App {
    /// Apply one service observation. A late response cannot replace another task's view.
    pub fn update(&mut self, message: Message) -> Option<Action> {
        match message {
            Message::Tasks(tasks) => return self.update_tasks(tasks),
            Message::Page(page) => {
                if self.selected.as_deref() != Some(&page.snapshot.task_id) {
                    return None;
                }
                if page.gap {
                    self.lines.clear();
                    self.transcript.clear();
                    self.last_sequence = None;
                    self.set_notice("Event history gap. Snapshot recovered; loading retained events. Full journal is on disk.".into());
                }
                for event in page.events {
                    if self
                        .last_sequence
                        .is_some_and(|last| event.sequence <= last)
                    {
                        continue;
                    }
                    self.append(&event);
                    self.last_sequence = Some(event.sequence);
                }
                if let Some(message) = &page.snapshot.message
                    && self.snapshot.as_ref().and_then(|old| old.message.as_ref()) != Some(message)
                {
                    self.set_notice(safe(message, 1024));
                }
                self.snapshot = Some(page.snapshot);
            }
            Message::Submitted(view) => return Some(self.submitted(view)),
            Message::Updated(view) => {
                self.acknowledge(&format!("task {}", view.task_id));
                if self.selected.as_deref() == Some(&view.task_id) {
                    self.snapshot = Some(view);
                }
                self.notice = "Command acknowledged. Task state comes from the service.".into();
                self.notice_level = NoticeLevel::Info;
            }
            Message::Connection { connected, detail } => {
                let changed = self.connected != connected;
                self.connected = connected;
                if (changed || !connected) && self.notice_level != NoticeLevel::Attention {
                    self.set_notice(safe(&detail, 512));
                    self.notice_level = if connected {
                        NoticeLevel::Info
                    } else {
                        NoticeLevel::Connection
                    };
                }
            }
            Message::Error(error) => self.set_notice(safe(&error, 1024)),
            Message::CommandError {
                reference,
                error,
                uncertain,
            } => {
                if uncertain {
                    if self.uncertain.len() == 16 {
                        self.uncertain.pop_front();
                    }
                    self.uncertain.push_back(reference.clone());
                }
                self.acknowledge(&reference);
                self.set_notice(safe(&error, 2048));
            }
        }
        None
    }

    fn submitted(&mut self, view: TaskView) -> Action {
        if self
            .submitted_prompt
            .as_ref()
            .is_some_and(|(id, text)| id == &view.request_id && text == self.composer.text.trim())
            || view
                .approval
                .as_ref()
                .is_some_and(|grant| grant.objective == self.composer.text.trim())
        {
            self.composer = Composer::default();
        }
        self.submitted_prompt = None;
        self.acknowledge(&format!("request {}", view.request_id));
        self.set_notice(
            if view.approval.is_some() {
                "Task prepared. Review the grant before execution."
            } else {
                "Task accepted. Its outcome will appear below."
            }
            .into(),
        );
        self.notice_level = NoticeLevel::Info;
        self.show_tasks = false;
        let action = self.select(view.task_id.clone());
        if let Some(grant) = &view.approval {
            self.transcript.push_back(Entry {
                kind: EntryKind::User,
                text: safe(&grant.objective, 4096),
                operation_id: None,
            });
        }
        self.snapshot = Some(view);
        if self
            .snapshot
            .as_ref()
            .is_some_and(|view| view.approval.is_some())
        {
            self.open_review();
        }
        action
    }

    fn update_tasks(&mut self, tasks: Vec<TaskSummary>) -> Option<Action> {
        self.tasks = tasks;
        if let Some(prefix) = self.selected.clone()
            && prefix.len() < 64
            && !self.tasks.iter().any(|task| task.task_id == prefix)
        {
            let matches = self
                .tasks
                .iter()
                .filter(|task| task.task_id.starts_with(&prefix))
                .collect::<Vec<_>>();
            if let [task] = matches.as_slice() {
                return Some(self.select(task.task_id.clone()));
            }
            self.selected = None;
            self.set_notice("Task prefix has no unique match. Use /tasks to select a task.".into());
            return None;
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
        if self.exit_intent == ExitIntent::WaitingForCommands {
            if key.code == KeyCode::Esc {
                self.exit_intent = ExitIntent::Attached;
            }
            return None;
        }
        if self.modal.is_some() {
            return self.modal_key(key);
        }
        if self.show_tasks {
            return self.task_key(key.code);
        }
        self.prompt_key(key)
    }

    fn task_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Esc => self.show_tasks = false,
            KeyCode::Enter => {
                self.show_tasks = false;
                if self.selected.is_none() {
                    return self.navigate(0);
                }
            }
            KeyCode::Down => return self.navigate(1),
            KeyCode::Up => return self.navigate(-1),
            _ => (),
        }
        None
    }

    pub fn open_code(&mut self, path: String) {
        let cursor = path.len();
        self.modal = Some(Modal::Code {
            fields: [path, self.default_model.clone(), self.account.clone()],
            focus: 0,
            cursor,
        });
    }

    /// Retain identities until the service acknowledges each queued mutation.
    pub fn queued(&mut self, reference: String) {
        self.pending.push_back(reference);
    }
    fn acknowledge(&mut self, reference: &str) {
        if let Some(index) = self.pending.iter().position(|item| item == reference) {
            self.pending.remove(index);
        }
        if self.exit_intent == ExitIntent::WaitingForCommands && self.pending.is_empty() {
            self.quit = true;
        }
    }

    pub fn paste(&mut self, text: &str) {
        match &mut self.modal {
            Some(Modal::Code {
                fields,
                focus,
                cursor,
            }) => input::paste(fields, *focus, cursor, text),
            Some(Modal::Scope { files, cursor, .. }) => {
                input::insert(files, cursor, text, 512, false);
            }
            None if !self.show_tasks => self.composer.paste(text),
            _ => (),
        }
    }

    /// Select a task and start its event cursor independently from another task.
    pub fn select(&mut self, id: String) -> Action {
        self.selected = Some(id.clone());
        self.snapshot = None;
        self.lines.clear();
        self.transcript.clear();
        self.last_sequence = None;
        self.conversation = None;
        self.scroll = 0;
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
            .saturating_add_signed(if self.selected.is_none() {
                0
            } else {
                direction
            })
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
                summary: fishnet_service::display::approval_summary(grant),
                expanded: false,
                scroll: 0,
            });
        } else {
            self.notice = "No pending approval for the selected task.".into();
        }
    }

    fn modal_key(&mut self, event: KeyEvent) -> Option<Action> {
        let key = event.code;
        let mut modal = self.modal.take()?;
        if key == KeyCode::Esc {
            return None;
        }
        let action = match &mut modal {
            Modal::Code {
                fields,
                focus,
                cursor,
            } => {
                if key == KeyCode::Enter {
                    return self.submit_code(fields);
                }
                input::edit(fields, focus, cursor, event);
                None
            }
            Modal::Scope {
                objective,
                files,
                cursor,
            } => {
                if key == KeyCode::Enter {
                    return self.submit_draft(objective.clone(), files);
                }
                input::edit_text(files, cursor, event, 512);
                None
            }
            Modal::Review {
                id,
                revision,
                scroll,
                expanded,
                ..
            } => match key {
                KeyCode::F(2) => {
                    *expanded = !*expanded;
                    *scroll = 0;
                    None
                }
                KeyCode::Enter | KeyCode::Char('y') => Some(Action::Approve {
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
                _ => {
                    scroll_dialog(scroll, key);
                    None
                }
            },
            Modal::Cancel(id) => {
                if key == KeyCode::Char('y') {
                    Some(Action::Cancel(id.clone()))
                } else {
                    return None;
                }
            }
            Modal::Notice { scroll, .. } | Modal::Help { scroll } => {
                scroll_dialog(scroll, key);
                None
            }
        };
        if action.is_none() {
            self.modal = Some(modal);
        }
        action
    }

    fn submit_draft(&mut self, objective: String, files: &str) -> Option<Action> {
        let files = files
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(String::from)
            .collect();
        match fishnet_service::new_id() {
            Ok(request_id) => Some(Action::Submit(Submission {
                request_id,
                task: TaskRequest::Rust {
                    objective,
                    files,
                    model: if self.default_model.is_empty() {
                        None
                    } else {
                        Some(self.default_model.clone())
                    },
                    account: self.account.clone(),
                },
            })),
            Err(error) => {
                self.set_notice(error.to_string());
                None
            }
        }
    }

    fn details(&self) -> String {
        let Some(view) = &self.snapshot else {
            return String::new();
        };
        let mut text = format!("Task: {}\nRequest: {}\n\n", view.task_id, view.request_id);
        if let Some(run) = &view.run {
            let _ = write!(
                text,
                "Model requests: {}\nTool calls: {}\nProcesses: {}\nRejected calls: {}\nPending operations: {}\n\n",
                run.statistics.provider_attempts,
                run.statistics.logical_tool_calls,
                run.statistics.process_attempts,
                run.statistics.rejected_calls,
                run.pending_operation_ids.len()
            );
            let stats = &run.statistics;
            let _ = write!(
                text,
                "Measured time (microseconds)\nProvider: {}\nTools: {}\nChecks: {}\nHarness: {}\nApproval wait (milliseconds): {}\n\n",
                stats.provider_us,
                stats.tool_us,
                stats.check_us,
                stats
                    .harness_us
                    .map_or_else(|| "unknown".into(), |value| value.to_string()),
                view.approval_wait_ms
                    .map_or_else(|| "unknown".into(), |value| value.to_string())
            );
            if let Some(outcome) = &run.outcome {
                let _ = write!(text, "{}\n\n", safe(&outcome.reason, 2048));
            }
        }
        text
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
                cursor: path.len(),
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
        self.notice_level = NoticeLevel::Attention;
        self.notice.clone_from(&notice);
        if self.notices.back() != Some(&notice) {
            self.notices.push_back(notice);
        }
        if self.notices.len() > 16 {
            self.notices.pop_front();
        }
    }

    fn append(&mut self, event: &Event) {
        if let fishnet_core::protocol::EventData::RunCreated { contract, .. } = &event.data {
            self.conversation = contract
                .conversation
                .as_ref()
                .map(|context| context.root_task_id.clone());
            self.mode = if self.conversation.is_some() {
                Mode::Chat
            } else {
                Mode::Code
            };
            self.transcript.clear();
        }
        transcript::append(&mut self.transcript, event, self.conversation.is_some());
        let description = describe(&event.data);
        let mut lines = description.lines();
        for line in lines.by_ref().take(24) {
            self.lines.push_back(if line.len() > 512 {
                format!("{}… [line preview truncated]", safe(line, 460))
            } else {
                safe(line, 512)
            });
        }
        if lines.next().is_some() {
            self.lines
                .push_back("[Preview truncated. Full output remains in the journal.]".into());
        }
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }
}

const fn scroll_dialog(scroll: &mut u16, key: KeyCode) {
    match key {
        KeyCode::Down => *scroll = scroll.saturating_add(1),
        KeyCode::Up => *scroll = scroll.saturating_sub(1),
        KeyCode::PageDown => *scroll = scroll.saturating_add(5),
        KeyCode::PageUp => *scroll = scroll.saturating_sub(5),
        KeyCode::Home => *scroll = 0,
        KeyCode::End => *scroll = u16::MAX,
        _ => (),
    }
}
