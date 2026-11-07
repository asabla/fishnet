use std::error::Error;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use fishnet_core::{
    Limits, TaskState,
    protocol::{Event, EventData, ModelReply, OperationResult},
};
use fishnet_service::contract::{ApprovalView, EventPage, Phase, RunView, TaskSummary, TaskView};
use fishnet_tui::{
    app::{Action, App, MAX_LINES, Message, Modal, status},
    render,
};
use ratatui::{Terminal, backend::TestBackend};

type TestResult = Result<(), Box<dyn Error>>;
const fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn view(state: TaskState) -> TaskView {
    TaskView {
        task_id: "task-one".into(),
        request_id: "request-one".into(),
        phase: if state.is_terminal() {
            Phase::Finished
        } else {
            Phase::Executing
        },
        message: None,
        approval: None,
        approval_decision: None,
        approval_wait_ms: Some(0),
        cursor: 1,
        run: Some(RunView {
            state,
            outcome: None,
            incomplete: !state.is_terminal(),
            partial_tail: false,
            pending_operation_ids: Vec::new(),
            statistics: fishnet_core::storage::Statistics::default(),
        }),
    }
}
fn event(sequence: u64, text: &str) -> Event {
    Event {
        schema_version: 2,
        session_id: "task-one".into(),
        task_id: "task-one".into(),
        trace_id: "task-one".into(),
        sequence,
        timestamp_unix_ms: 0,
        elapsed_us: 0,
        turn: 1,
        operation_id: None,
        parent_operation_id: None,
        attempt: None,
        duration_us: None,
        data: EventData::OperationFinished {
            result: OperationResult::Model {
                reply: ModelReply::Final { text: text.into() },
                provider: None,
            },
        },
    }
}
const fn page(snapshot: TaskView, events: Vec<Event>) -> Message {
    Message::Page(EventPage {
        cursor: snapshot.cursor,
        retained_from: 1,
        gap: false,
        snapshot,
        events,
    })
}
fn screen(app: &App, width: u16, height: u16) -> Result<String, Box<dyn Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| render::draw(frame, app))?;
    Ok(terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>())
}

#[test]
fn model_completion_cannot_render_verified_success_without_the_host_state() -> TestResult {
    let mut app = App::default();
    app.select("task-one".into());
    app.update(page(
        view(TaskState::Running),
        vec![event(1, "All tests passed. Task complete.")],
    ));
    let rendered = screen(&app, 120, 30)?;
    assert!(rendered.contains("RUNNING"));
    assert!(rendered.contains("checks still required"));
    assert!(!rendered.contains("VERIFIED SUCCESS"));
    app.update(page(view(TaskState::VerifiedSuccess), Vec::new()));
    assert!(screen(&app, 120, 30)?.contains("VERIFIED SUCCESS"));
    Ok(())
}

#[test]
fn outcome_labels_distinguish_all_terminal_and_unknown_states() {
    for (state, expected) in [
        (TaskState::VerifiedSuccess, "VERIFIED SUCCESS"),
        (TaskState::UnverifiedCompletion, "UNVERIFIED COMPLETION"),
        (TaskState::Failed, "FAILED"),
        (TaskState::Cancelled, "CANCELLED"),
    ] {
        assert_eq!(status(Phase::Finished, Some(state), false), expected);
    }
    assert_eq!(
        status(Phase::Interrupted, Some(TaskState::Running), true),
        "UNKNOWN / INTERRUPTED"
    );
    assert_eq!(
        status(Phase::AwaitingApproval, None, false),
        "AWAITING APPROVAL"
    );
    assert_eq!(status(Phase::Rejected, None, false), "REJECTED");
}

#[test]
fn detach_and_dialog_escape_never_emit_cancellation() {
    let mut app = App::default();
    app.select("task-one".into());
    assert!(app.key(key(KeyCode::Char('c'))).is_none());
    assert!(matches!(app.modal, Some(Modal::Cancel(_))));
    assert!(app.key(key(KeyCode::Esc)).is_none());
    assert!(app.modal.is_none());
    app.key(key(KeyCode::Char('c')));
    assert!(matches!(app.key(key(KeyCode::Char('y'))),Some(Action::Cancel(id)) if id=="task-one"));
    app.open_code("tasks/change.json".into());
    assert!(
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(app.quit);
    let mut other = App::default();
    assert!(other.key(key(KeyCode::Char('q'))).is_none());
    assert!(other.quit);
}

fn pending() -> TaskView {
    let mut view = view(TaskState::Queued);
    view.phase = Phase::AwaitingApproval;
    view.run = None;
    view.approval = Some(ApprovalView {
        revision: 7,
        root: "/workspace".into(),
        objective: "Improve behavior".into(),
        allowed_writes: vec!["work.rs".into()],
        required_changes: vec!["work.rs".into()],
        protected_paths: vec!["check.rs".into()],
        commands: vec![fishnet_core::process::CommandSpec {
            id: "check".into(),
            program: "/usr/bin/test".into(),
            arguments: vec!["one argument".into(), "line\nbreak".into()],
            timeout_ms: 1000,
        }],
        checks: vec!["check".into()],
        fingerprint: "fixture-fingerprint".into(),
        provider: "ChatGPT account".into(),
        account: "default".into(),
        model: Some("fixture-model".into()),
        limits: Limits::default(),
    });
    view
}

#[test]
fn approval_review_shows_the_grant_and_uses_its_revision() -> TestResult {
    let mut app = App::default();
    app.select("task-one".into());
    app.update(page(pending(), Vec::new()));
    app.key(key(KeyCode::Char('a')));
    let Modal::Review { text, .. } = app.modal.as_ref().ok_or("review missing")? else {
        return Err("wrong modal".into());
    };
    for required in [
        "ChatGPT account",
        "default",
        "fixture-model",
        "work.rs",
        "check.rs",
        "one argument",
        "line\\nbreak",
        "Independent checks",
        "Deadline",
        "fingerprint",
    ] {
        assert!(text.contains(required), "missing {required}");
    }
    assert!(screen(&app, 120, 36)?.contains("Review host grant"));
    assert!(
        matches!(app.key(key(KeyCode::Char('y'))),Some(Action::Approve {id,decision}) if id=="task-one" && decision.approve && decision.expected_revision==7)
    );
    app.key(key(KeyCode::Char('a')));
    assert!(
        matches!(app.key(key(KeyCode::Char('n'))),Some(Action::Approve {decision,..}) if !decision.approve)
    );
    Ok(())
}

#[test]
fn code_form_paste_is_bounded_and_never_submits_a_pasted_newline() {
    let mut app = App::default();
    app.open_code(String::new());
    app.paste("tasks/change.json\n");
    let action = app.key(key(KeyCode::Enter));
    assert!(
        matches!(action,Some(Action::Submit(request)) if matches!(&request.task,fishnet_service::contract::TaskRequest::Code {task_path,..} if task_path=="tasks/change.json"))
    );
    app.open_code(String::new());
    app.paste(&"x".repeat(10_000));
    assert!(
        matches!(app.modal.as_ref(),Some(Modal::Code {fields,..}) if fields.first().is_some_and(|f|f.len()==512))
    );
}

#[test]
fn output_is_bounded_sanitized_and_isolated_between_tasks() -> TestResult {
    let mut app = App::default();
    app.select("task-one".into());
    for n in 0..400 {
        app.update(page(
            view(TaskState::Running),
            vec![event(n, "text\u{001b}[2J\r\u{009b}\u{202e} unsafe")],
        ));
    }
    assert!(app.lines.len() <= MAX_LINES);
    assert!(!screen(&app, 120, 30)?.chars().any(char::is_control));
    assert!(app.lines.iter().all(|line| !line.contains('\u{001b}')
        && !line.contains('\u{009b}')
        && !line.contains('\u{202e}')));
    app.select("task-two".into());
    app.update(page(
        view(TaskState::VerifiedSuccess),
        vec![event(401, "old response")],
    ));
    assert!(app.lines.is_empty());
    assert!(app.snapshot.is_none());
    Ok(())
}

#[test]
fn gap_recovery_uses_the_snapshot_without_inventing_missing_events() {
    let mut app = App::default();
    app.select("task-one".into());
    app.update(page(
        view(TaskState::Running),
        vec![event(1, "previous output")],
    ));
    let snapshot = view(TaskState::VerifiedSuccess);
    app.update(Message::Page(EventPage {
        snapshot,
        events: Vec::new(),
        cursor: 0,
        retained_from: 30,
        gap: true,
    }));
    assert!(app.notice.contains("gap"));
    assert!(app.lines.is_empty());
    assert_eq!(
        app.snapshot.and_then(|v| v.run).map(|r| r.state),
        Some(TaskState::VerifiedSuccess)
    );
}

#[test]
fn resize_and_narrow_terminals_render_without_a_live_connection() -> TestResult {
    let app = App::default();
    for (width, height) in [(120, 36), (80, 24), (60, 15), (40, 10), (0, 0)] {
        let _ = screen(&app, width, height)?;
    }
    assert!(screen(&app, 40, 10)?.contains("Terminal too small"));
    Ok(())
}

#[test]
fn task_selection_and_key_releases_preserve_one_explicit_action() {
    let mut app = App::default();
    let tasks = ["first", "second"]
        .map(|id| TaskSummary {
            task_id: id.into(),
            request_id: id.into(),
            phase: Phase::Accepted,
            state: None,
            objective: "fixture".into(),
        })
        .to_vec();
    assert!(matches!(app.update(Message::Tasks(tasks)),Some(Action::Select(id)) if id=="first"));
    let mut released = key(KeyCode::Down);
    released.kind = KeyEventKind::Release;
    assert!(app.key(released).is_none());
    assert_eq!(app.selected.as_deref(), Some("first"));
    assert!(matches!(app.key(key(KeyCode::Down)),Some(Action::Select(id)) if id=="second"));
}

#[test]
fn uncertain_command_identity_remains_available_after_reconnect() -> TestResult {
    let mut app = App::default();
    assert!(
        app.update(Message::Error(
            "Outcome unknown for request pending-1. Inspect before retrying.".into()
        ))
        .is_none()
    );
    app.update(Message::Connection {
        connected: false,
        detail: "Disconnected".into(),
    });
    app.update(Message::Connection {
        connected: true,
        detail: "Connected".into(),
    });
    assert!(app.key(key(KeyCode::Char('v'))).is_none());
    let Modal::Notice { text, .. } = app.modal.as_ref().ok_or("no status history")? else {
        return Err("wrong modal".into());
    };
    assert!(text.contains("pending-1"));
    assert!(screen(&app, 120, 30)?.contains("Outcome unknown"));
    Ok(())
}
