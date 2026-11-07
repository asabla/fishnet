use std::error::Error;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fishnet_core::{
    Event,
    protocol::{EventData, ModelReply, OperationRequest, OperationResult, ToolCall, ToolResult},
};
use fishnet_service::contract::{EventPage, Phase, TaskView};
use fishnet_tui::app::{Action, App, EntryKind, MAX_ENTRIES, Message, Modal};

type TestResult = Result<(), Box<dyn Error>>;

fn press(app: &mut App, code: KeyCode) -> Option<Action> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn ordinary_keys_are_text_and_only_enter_runs_a_slash_command() {
    let mut app = App::default();
    for ch in "nfpacqrv?jk".chars() {
        assert!(press(&mut app, KeyCode::Char(ch)).is_none());
    }
    assert_eq!(app.composer.text, "nfpacqrv?jk");
    assert!(!app.quit);
    assert!(app.modal.is_none());
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.paste("/fixture\n");
    assert!(app.snapshot.is_none());
    assert!(matches!(
        press(&mut app, KeyCode::Enter),
        Some(Action::Submit(_))
    ));
    assert!(!app.quit);
}

#[test]
fn prompt_preserves_multiline_intent_through_scope_and_cancel() -> TestResult {
    let mut app = App::default();
    app.paste("/code");
    press(&mut app, KeyCode::Enter);
    app.paste("Fix the result.\nKeep the existing test.");
    assert!(press(&mut app, KeyCode::Enter).is_none());
    assert!(matches!(app.modal, Some(Modal::Scope { .. })));
    app.paste("src/lib.rs");
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        app.composer.text,
        "Fix the result.\nKeep the existing test."
    );
    press(&mut app, KeyCode::Enter);
    app.paste("src/lib.rs");
    let Some(Action::Submit(request)) = press(&mut app, KeyCode::Enter) else {
        return Err("scope did not prepare a submission".into());
    };
    let fishnet_service::contract::TaskRequest::Rust {
        objective, files, ..
    } = request.task
    else {
        return Err("wrong task kind".into());
    };
    assert_eq!(objective, "Fix the result.\nKeep the existing test.");
    assert_eq!(files, ["src/lib.rs"]);
    assert!(app.snapshot.is_none());
    assert!(!app.composer.text.is_empty());
    Ok(())
}

#[test]
fn prompt_is_bounded_sanitized_and_commands_can_be_completed() {
    let mut app = App::default();
    app.paste("hello\u{001b}\u{202e}\u{009b}\nworld");
    assert!(
        !app.composer
            .text
            .contains(['\u{001b}', '\u{202e}', '\u{009b}'])
    );
    app.paste(&"界".repeat(4096));
    assert!(app.composer.text.len() <= 4096);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Backspace);
    assert!(app.composer.text.is_char_boundary(app.composer.cursor));
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.paste("/det");
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.composer.text, "/details");
    assert!(app.modal.is_none());
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.modal, Some(Modal::Notice { .. })));
}

fn event(sequence: u64, operation_id: Option<&str>, data: EventData) -> Event {
    Event {
        schema_version: 2,
        session_id: "task".into(),
        task_id: "task".into(),
        trace_id: "task".into(),
        sequence,
        timestamp_unix_ms: 0,
        elapsed_us: 0,
        turn: 1,
        operation_id: operation_id.map(String::from),
        parent_operation_id: None,
        attempt: None,
        duration_us: Some(2000),
        data,
    }
}

fn page(events: Vec<Event>) -> Message {
    Message::Page(EventPage {
        cursor: events.last().map_or(0, |event| event.sequence),
        retained_from: 1,
        gap: false,
        events,
        snapshot: TaskView {
            task_id: "task".into(),
            request_id: "request".into(),
            phase: Phase::Executing,
            message: None,
            approval: None,
            approval_decision: None,
            approval_wait_ms: None,
            cursor: 0,
            run: None,
        },
    })
}

#[test]
fn transcript_groups_operation_results_and_keeps_protocol_noise_in_details() {
    let mut app = App::default();
    app.select("task".into());
    app.update(page(vec![
        event(
            1,
            Some("model"),
            EventData::OperationStarted {
                request: OperationRequest::Provider {
                    provider: "fixture".into(),
                    previous_tool_operation_id: None,
                },
            },
        ),
        event(
            2,
            Some("tool"),
            EventData::OperationStarted {
                request: OperationRequest::Tool {
                    call: ToolCall {
                        name: "read_file".into(),
                        arguments: serde_json::json!({"path":"src/lib.rs"}),
                    },
                    relevant_state: "revision".into(),
                    effective_command: None,
                },
            },
        ),
        event(
            3,
            Some("tool"),
            EventData::OperationFinished {
                result: OperationResult::Tool {
                    result: ToolResult::Found {
                        key: "answer".into(),
                        value: "42".into(),
                        revision: "revision".into(),
                    },
                },
            },
        ),
        event(
            4,
            Some("model"),
            EventData::OperationFinished {
                result: OperationResult::Model {
                    reply: ModelReply::Final {
                        text: "The file is ready.".into(),
                    },
                    provider: None,
                },
            },
        ),
    ]));
    assert_eq!(app.transcript.len(), 2);
    let tool = app.transcript.front();
    assert!(tool.is_some_and(|entry| entry.kind == EntryKind::Tool
        && entry.text.contains("read_file src/lib.rs")
        && entry.text.contains("answer = 42")
        && entry.text.contains("2 ms")));
    assert!(app.lines.iter().any(|line| line == "Model request"));
    app.paste("/details");
    press(&mut app, KeyCode::Enter);
    assert!(
        matches!(&app.modal, Some(Modal::Notice { text, .. }) if text.contains("Model request"))
    );
}

#[test]
fn transcript_bounds_and_task_selection_do_not_discard_the_prompt() {
    let mut app = App::default();
    app.select("task".into());
    app.paste("Unsent work");
    for sequence in 1..=200 {
        app.update(page(vec![event(
            sequence,
            None,
            EventData::OperationFinished {
                result: OperationResult::Model {
                    reply: ModelReply::Final {
                        text: "x".repeat(8192),
                    },
                    provider: None,
                },
            },
        )]));
    }
    assert_eq!(app.transcript.len(), MAX_ENTRIES);
    assert!(
        app.transcript
            .iter()
            .all(|entry| entry.text.len() < 4200 && entry.text.contains("preview truncated"))
    );
    app.select("another".into());
    assert!(app.transcript.is_empty());
    assert_eq!(app.composer.text, "Unsent work");
}

#[test]
fn replayed_observations_and_submission_acknowledgement_preserve_new_input() -> TestResult {
    let mut app = App::default();
    app.select("task".into());
    let observation = event(
        1,
        None,
        EventData::OperationFinished {
            result: OperationResult::Model {
                reply: ModelReply::Final {
                    text: "A response".into(),
                },
                provider: None,
            },
        },
    );
    app.update(page(vec![observation.clone()]));
    app.update(page(vec![observation]));
    assert_eq!(app.transcript.len(), 1);
    app.paste("A new unsent task");
    let Message::Page(acknowledgement) = page(Vec::new()) else {
        return Err("invalid fixture".into());
    };
    app.update(Message::Submitted(acknowledgement.snapshot));
    assert_eq!(app.composer.text, "A new unsent task");
    Ok(())
}

#[test]
fn plain_message_submits_chat_without_file_selection_and_can_continue_a_saved_turn() -> TestResult {
    let mut app = App::default();
    app.paste("hello");
    let Some(Action::Submit(request)) = press(&mut app, KeyCode::Enter) else {
        return Err("no chat submission".into());
    };
    assert!(matches!(
        request.task,
        fishnet_service::contract::TaskRequest::Chat {
            parent_task_id: None,
            ..
        }
    ));
    assert!(app.modal.is_none());
    app.composer = fishnet_tui::app::Composer::default();
    app.select("task".into());
    let context = fishnet_core::conversation::Context {
        schema_version: 1,
        root_task_id: "root".into(),
        messages: vec![
            fishnet_core::conversation::Message {
                role: fishnet_core::conversation::Role::User,
                text: "Remember Ada".into(),
            },
            fishnet_core::conversation::Message {
                role: fishnet_core::conversation::Role::Assistant,
                text: "Understood".into(),
            },
            fishnet_core::conversation::Message {
                role: fishnet_core::conversation::Role::User,
                text: "Which name?".into(),
            },
        ],
    };
    let mut message = page(vec![event(
        1,
        None,
        EventData::RunCreated {
            contract: context.into_contract(),
            limits: fishnet_core::conversation::limits(),
            provider_version: "fixture".into(),
            workspace_start: None,
        },
    )]);
    if let Message::Page(page) = &mut message {
        page.snapshot.phase = Phase::Finished;
    }
    app.update(message);
    assert_eq!(app.conversation.as_deref(), Some("root"));
    assert_eq!(app.transcript.len(), 3);
    assert_eq!(
        app.transcript.get(1).map(|entry| entry.text.as_str()),
        Some("Understood")
    );
    app.paste("Continue");
    assert!(
        matches!(press(&mut app,KeyCode::Enter),Some(Action::Submit(request)) if matches!(&request.task,fishnet_service::contract::TaskRequest::Chat {parent_task_id:Some(id),..} if id=="task"))
    );
    Ok(())
}

#[test]
fn active_turn_keeps_new_input_until_the_response_arrives() {
    let mut app = App::default();
    app.select("task".into());
    app.update(page(Vec::new()));
    app.conversation = Some("root".into());
    app.paste("Do not lose this message");
    assert!(press(&mut app, KeyCode::Enter).is_none());
    assert_eq!(app.composer.text, "Do not lose this message");
    assert!(app.modal.is_none());
}
