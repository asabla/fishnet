pub mod support;

use std::error::Error;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fishnet_core::TaskState;
use fishnet_service::contract::{Phase, RunView, TaskSummary, TaskView};
use fishnet_tui::{
    app::{Action, App, Entry, EntryKind, Modal},
    render,
};
use ratatui::{Terminal, backend::TestBackend};

type TestResult = Result<(), Box<dyn Error>>;
fn key(app: &mut App, code: KeyCode) -> Option<Action> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn draw(app: &mut App, width: u16, height: u16) -> Result<String, Box<dyn Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| render::draw(frame, app))?;
    support::export(app, terminal.backend().buffer())?;
    Ok(terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width))
        .map(|row| {
            row.iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
}
fn snapshot(name: &str, rendered: &str) -> TestResult {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/screens")
        .join(format!("{name}.txt"));
    if std::env::var_os("FISHNET_UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
        std::fs::write(&path, rendered)?;
    }
    assert_eq!(
        rendered,
        std::fs::read_to_string(path)?,
        "layout {name} changed; review the rendered rows before updating snapshots"
    );
    Ok(())
}
fn running() -> App {
    let mut app = App::default();
    app.update(fishnet_tui::app::Message::Connection {
        connected: true,
        detail: "Enter send · / commands · Ctrl-P tasks · Ctrl-C detach.".into(),
    });
    let id = "a".repeat(64);
    app.tasks = vec![TaskSummary {
        task_id: id.clone(),
        request_id: "request-one".into(),
        phase: Phase::Executing,
        state: Some(TaskState::Running),
        objective: "Improve terminal rendering".into(),
    }];
    app.selected = Some(id.clone());
    app.snapshot = Some(TaskView {
        task_id: id,
        request_id: "request-one".into(),
        phase: Phase::Executing,
        message: None,
        approval: None,
        approval_decision: Some(true),
        approval_wait_ms: Some(0),
        cursor: 3,
        run: Some(RunView {
            state: TaskState::Running,
            outcome: None,
            incomplete: true,
            partial_tail: false,
            pending_operation_ids: Vec::new(),
            statistics: fishnet_core::storage::Statistics::default(),
        }),
    });
    app.transcript = [
        (EntryKind::User, "Improve terminal rendering"),
        (EntryKind::Tool, "read_file apps/tui/src/render.rs\nFile result: apps/tui/src/render.rs  (2 ms)"),
        (EntryKind::Assistant, "The task view now keeps the prompt visible while you read earlier output.\n\nTool details remain available through /details."),
        (EntryKind::Check, "Independent check · format\nPassed  (12 ms)"),
    ].into_iter().map(|(kind, text)| Entry { kind, text: text.into(), operation_id: None }).collect();

    app
}

#[test]
fn reviewed_layouts_cover_normal_and_small_terminals() -> TestResult {
    let mut welcome = App::default();
    welcome.update(fishnet_tui::app::Message::Connection {
        connected: true,
        detail: "Local service ready. No provider request has been made.".into(),
    });
    snapshot("welcome-80x24", &draw(&mut welcome, 80, 24)?)?;
    let mut app = running();
    snapshot("running-120x30", &draw(&mut app, 120, 30)?)?;
    snapshot("running-80x24", &draw(&mut app, 80, 24)?)?;
    app.paste("Wrap long output and keep approval controls visible when the terminal is resized.");
    key(&mut app, KeyCode::Enter);
    app.paste("apps/tui/src/render.rs");
    snapshot("new-task-80x24", &draw(&mut app, 80, 24)?)?;
    let narrow = draw(&mut app, 40, 15)?;
    assert!(narrow.contains("Files to change"));
    assert!(narrow.contains("Enter Review"));
    snapshot("new-task-40x15", &narrow)?;
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::F(1));
    key(&mut app, KeyCode::End);
    let help = draw(&mut app, 40, 15)?;
    assert!(help.contains("roll them back"));
    assert!(help.contains("Esc Close"));
    snapshot("help-end-40x15", &help)?;
    let Some(Modal::Help { scroll: before }) = app.modal else {
        return Err("missing help".into());
    };
    key(&mut app, KeyCode::Up);
    assert!(matches!(app.modal,Some(Modal::Help {scroll}) if scroll==before.saturating_sub(1)));
    Ok(())
}

#[test]
fn wrapped_output_is_readable_at_the_end_and_can_scroll_to_the_start() -> TestResult {
    let mut app = running();
    app.transcript = std::iter::once(Entry {
        kind: EntryKind::Assistant,
        text: format!("FIRST {} FINAL_VISIBLE", "a readable result ".repeat(50)),
        operation_id: None,
    })
    .collect();
    let last = draw(&mut app, 40, 15)?;
    assert!(last.contains("FINAL_VISIBLE"));
    assert!(!last.contains("FIRST"));
    for _ in 0..20 {
        key(&mut app, KeyCode::PageUp);
    }
    assert!(draw(&mut app, 40, 15)?.contains("FIRST"));
    key(&mut app, KeyCode::Esc);
    assert!(draw(&mut app, 40, 15)?.contains("FINAL_VISIBLE"));
    Ok(())
}

#[test]
fn new_task_edits_unicode_in_place_and_submits_intent_without_executing_it() -> TestResult {
    let mut app = App::default();
    app.paste("Fix 界x");
    key(&mut app, KeyCode::Left);
    key(&mut app, KeyCode::Backspace);
    app.paste("rendering ");
    key(&mut app, KeyCode::Enter);
    app.paste("apps/tui/src/render.rs, apps/tui/src/app.rs");
    let action = key(&mut app, KeyCode::Enter).ok_or("no submission")?;
    let Action::Submit(request) = action else {
        return Err("wrong action".into());
    };
    let fishnet_service::contract::TaskRequest::Rust {
        objective, files, ..
    } = request.task
    else {
        return Err("wrong task kind".into());
    };
    assert_eq!(objective, "Fix rendering x");
    assert_eq!(files, vec!["apps/tui/src/render.rs", "apps/tui/src/app.rs"]);
    assert!(app.snapshot.is_none());
    Ok(())
}

#[test]
fn approval_controls_and_outcome_labels_remain_readable_after_resize() -> TestResult {
    let mut app = running();
    let grant = fishnet_service::contract::ApprovalView {
        revision: 1,
        root: "/workspace/fishnet".into(),
        objective: "Keep the complete grant readable on resize".into(),
        allowed_writes: vec!["apps/tui/src/render.rs".into()],
        required_changes: vec!["apps/tui/src/render.rs".into()],
        protected_paths: vec!["tests/behavior.rs".into()],
        commands: vec![fishnet_core::process::CommandSpec {
            id: "test".into(),
            program: "/usr/bin/cargo".into(),
            arguments: vec![
                "test".into(),
                "--workspace".into(),
                "--offline".into(),
                "--locked".into(),
            ],
            timeout_ms: 120_000,
        }],
        checks: vec!["test".into()],
        fingerprint: "reviewed-grant-fingerprint".into(),
        provider: "ChatGPT account".into(),
        account: "default".into(),
        model: Some("fixture-model".into()),
        limits: fishnet_core::Limits::default(),
    };
    let view = app.snapshot.as_mut().ok_or("missing view")?;
    view.phase = Phase::AwaitingApproval;
    view.run = None;
    view.approval = Some(grant);
    app.paste("/review");
    key(&mut app, KeyCode::Enter);
    for (width, height) in [(40, 12), (60, 15), (80, 24), (120, 36)] {
        let rendered = draw(&mut app, width, height)?;
        assert!(rendered.contains("y Approve  n Reject  Esc Close"));
        assert!(!rendered.contains("RUNNING"));
    }
    snapshot("approval-80x24", &draw(&mut app, 80, 24)?)?;
    key(&mut app, KeyCode::End);
    let end = draw(&mut app, 80, 24)?;
    snapshot("approval-end-80x24", &end)?;
    assert!(end.contains("Approval permits source and command output"));
    assert!(end.contains("provider."));
    assert!(end.contains("host OS permissions"));
    key(&mut app, KeyCode::Esc);
    let view = app.snapshot.as_mut().ok_or("missing view")?;
    view.phase = Phase::Interrupted;
    view.approval = None;
    assert!(draw(&mut app, 40, 12)?.contains("UNKNOWN / INTERRUPTED"));
    Ok(())
}

#[test]
fn prompt_and_command_menu_remain_visible_after_editing_and_resize() -> TestResult {
    let mut app = running();
    app.paste("Fix the result.\nKeep the existing test.");
    for (width, height) in [(40, 12), (80, 24), (120, 30)] {
        let screen = draw(&mut app, width, height)?;
        assert!(screen.contains("Fix the result."));
        assert!(screen.contains("Keep the existing test."));
        assert!(screen.contains("RUNNING"));
    }
    snapshot("multiline-80x24", &draw(&mut app, 80, 24)?)?;
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.paste("/rev");
    let menu = draw(&mut app, 40, 15)?;
    assert!(menu.contains("/review"));
    assert!(menu.contains("› /rev"));
    snapshot("commands-40x15", &menu)?;
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.composer.text, "/review");
    key(&mut app, KeyCode::PageUp);
    assert!(draw(&mut app, 80, 24)?.contains("› /review"));
    Ok(())
}

#[test]
fn prompt_grows_when_the_cursor_wraps_past_the_last_column() -> TestResult {
    let mut app = App::default();
    app.paste(&"x".repeat(36));
    let mut terminal = Terminal::new(TestBackend::new(40, 15))?;
    terminal.draw(|frame| render::draw(frame, &mut app))?;
    let rows = terminal
        .backend()
        .buffer()
        .content
        .chunks(40)
        .filter(|row| {
            row.first()
                .is_some_and(|cell| cell.bg == ratatui::style::Color::Rgb(32, 32, 32))
        })
        .count();
    assert_eq!(
        rows, 4,
        "the full prompt and its end cursor need two input rows"
    );
    Ok(())
}
