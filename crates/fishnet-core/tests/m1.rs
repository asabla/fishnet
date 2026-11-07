use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use fishnet_core::fixture::ScriptStep;
use fishnet_core::protocol::{
    ErrorKind, EventData, ModelReply, OperationRequest, OperationResult, ToolCall, ToolResult,
};
use fishnet_core::storage::{RunRecord, record_path};
use fishnet_core::tool::{self, TOOL_NAME};
use fishnet_core::{
    Cancellation, Limits, RunOptions, Scenario, ScriptedProvider, TaskState, inspect, run,
};
use serde_json::json;

type TestResult = Result<(), Box<dyn Error>>;
static NEXT_STORE: AtomicU64 = AtomicU64::new(0);

struct Store(PathBuf);

impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "fishnet-core-test-{}-{}",
            std::process::id(),
            NEXT_STORE.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn options(
        &self,
        scenario: Scenario,
    ) -> Result<(RunOptions, ScriptedProvider), Box<dyn Error>> {
        let (provider, contract) = ScriptedProvider::scenario(scenario, Duration::ZERO)?;
        Ok((
            RunOptions {
                store: self.0.clone(),
                limits: Limits::default(),
                contract,
                cancellation: Cancellation::default(),
            },
            provider,
        ))
    }

    fn execute(&self, scenario: Scenario) -> Result<RunRecord, Box<dyn Error>> {
        let (options, provider) = self.options(scenario)?;
        let result = run(options, provider, |_| ())?;
        Ok(inspect(&self.0, &result.run_id)?)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn scenarios_retain_success_unverified_and_failure_evidence() -> TestResult {
    for (scenario, state, providers, tools) in [
        (Scenario::Success, TaskState::VerifiedSuccess, 2, 1),
        (Scenario::Unverified, TaskState::UnverifiedCompletion, 1, 0),
        (Scenario::CheckFailure, TaskState::Failed, 2, 1),
        (
            Scenario::RepeatedInvalid,
            TaskState::UnverifiedCompletion,
            3,
            2,
        ),
        (Scenario::Negative, TaskState::VerifiedSuccess, 2, 1),
        (Scenario::ProviderFailure, TaskState::Failed, 1, 0),
    ] {
        let record = Store::new().execute(scenario)?;
        assert_eq!(record.state, state, "{scenario:?}");
        assert!(!record.incomplete);
        assert!(record.pending_operation_ids.is_empty());
        assert_eq!(record.statistics.provider_attempts, providers);
        assert_eq!(record.statistics.logical_tool_calls, tools);
        assert_eq!(record.outcome.ok_or("missing outcome")?.state, state);
    }
    Ok(())
}

#[test]
fn success_has_correlated_model_tool_result_model_and_check() -> TestResult {
    let store = Store::new();
    let (options, provider) = store.options(Scenario::Success)?;
    let mut observed = Vec::new();
    let result = run(options, provider, |event| observed.push(event.clone()))?;
    let record = inspect(&store.0, &result.run_id)?;
    assert_eq!(
        serde_json::to_value(&record.events)?,
        serde_json::to_value(&observed)?
    );
    let mut provider_ids = Vec::new();
    let mut tool_id = None;
    for event in &record.events {
        assert_eq!(event.session_id, result.run_id);
        if let EventData::OperationStarted { request } = &event.data {
            match request {
                OperationRequest::Provider {
                    previous_tool_operation_id,
                    ..
                } => {
                    if provider_ids.len() == 1 {
                        assert_eq!(previous_tool_operation_id, &tool_id);
                    }
                    provider_ids.push(event.operation_id.clone());
                }
                OperationRequest::Tool { .. } => {
                    assert_eq!(
                        event.parent_operation_id,
                        provider_ids.first().cloned().flatten()
                    );
                    tool_id.clone_from(&event.operation_id);
                }
                OperationRequest::Check { .. } => (),
            }
        }
        if matches!(event.data, EventData::OperationFinished { .. }) {
            assert_eq!(event.attempt, Some(1));
            assert!(event.duration_us.is_some());
        }
    }
    assert_eq!(provider_ids.len(), 2);
    assert!(tool_id.is_some());
    Ok(())
}

#[test]
fn final_claim_does_not_pass_independent_check() -> TestResult {
    let store = Store::new();
    let (options, _) = store.options(Scenario::Success)?;
    let provider = ScriptedProvider::new(
        vec![ScriptStep {
            expected_previous_result: None,
            reply: Ok(ModelReply::Final {
                text: "I verified everything successfully.".into(),
            }),
        }],
        Duration::ZERO,
    )?;
    let result = run(options, provider, |_| ())?;
    assert_eq!(result.outcome.state, TaskState::UnverifiedCompletion);
    Ok(())
}

#[test]
fn provider_must_receive_the_actual_tool_result() -> TestResult {
    let store = Store::new();
    let (options, _) = store.options(Scenario::Success)?;
    let provider = ScriptedProvider::new(
        vec![
            ScriptStep {
                expected_previous_result: None,
                reply: Ok(ModelReply::ToolCall(ToolCall {
                    name: TOOL_NAME.into(),
                    arguments: json!({"key": "greeting"}),
                })),
            },
            ScriptStep {
                expected_previous_result: None,
                reply: Ok(ModelReply::Final {
                    text: "done".into(),
                }),
            },
        ],
        Duration::ZERO,
    )?;
    let result = run(options, provider, |_| ())?;
    let record = inspect(&store.0, &result.run_id)?;
    assert_eq!(record.state, TaskState::Failed);
    assert!(
        record
            .outcome
            .ok_or("missing outcome")?
            .reason
            .contains("expected tool result")
    );
    Ok(())
}

#[test]
fn cancellation_before_start_and_at_operation_boundaries_is_terminal() -> TestResult {
    for boundary in ["before", "provider", "tool", "result", "check"] {
        let store = Store::new();
        let (options, provider) = store.options(Scenario::Success)?;
        let cancellation = options.cancellation.clone();
        if boundary == "before" {
            cancellation.cancel();
        }
        let result = run(options, provider, |event| {
            let cancel = matches!(
                (&event.data, boundary),
                (
                    EventData::OperationStarted {
                        request: OperationRequest::Provider { .. },
                    },
                    "provider",
                ) | (
                    EventData::OperationStarted {
                        request: OperationRequest::Tool { .. },
                    },
                    "tool",
                ) | (
                    EventData::OperationFinished {
                        result: OperationResult::Tool { .. },
                    },
                    "result",
                ) | (
                    EventData::OperationFinished {
                        result: OperationResult::Check { .. },
                    },
                    "check",
                )
            );
            if cancel {
                cancellation.cancel();
            }
        })?;
        let record = inspect(&store.0, &result.run_id)?;
        assert_eq!(record.state, TaskState::Cancelled, "{boundary}");
        assert!(record.pending_operation_ids.is_empty());
        assert!(record.events.iter().any(|e| matches!(
            e.data,
            EventData::StateChanged {
                to: TaskState::Cancelling,
                ..
            }
        )));
        if ["before", "provider", "tool"].contains(&boundary) {
            assert_eq!(record.statistics.tool_results, 0);
        }
    }
    Ok(())
}

#[test]
fn cancellation_wakes_a_waiting_provider() -> TestResult {
    let store = Store::new();
    let (mut options, _) = store.options(Scenario::Success)?;
    options.limits.deadline_ms = 300_000;
    let (provider, _) = ScriptedProvider::scenario(Scenario::Success, Duration::from_secs(300))?;
    let cancellation = options.cancellation.clone();
    let (ready, waiting) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        if waiting.recv().is_ok() {
            cancellation.cancel();
        }
    });
    let result = run(options, provider, |event| {
        if matches!(
            event.data,
            EventData::OperationStarted {
                request: OperationRequest::Provider { .. }
            }
        ) {
            let _ = ready.send(());
        }
    })?;
    thread.join().map_err(|_| "cancellation worker panicked")?;
    assert_eq!(result.outcome.state, TaskState::Cancelled);
    assert_eq!(
        inspect(&store.0, &result.run_id)?
            .statistics
            .logical_tool_calls,
        0
    );
    Ok(())
}

#[test]
fn deadline_is_a_recorded_failure_and_call_budget_is_bounded() -> TestResult {
    for (deadline_ms, delay_ms, calls, reason) in [
        (0, 0, 16, "DeadlineExceeded"),
        (20, 100, 16, "DeadlineExceeded"),
        (5_000, 0, 1, "CallLimit"),
    ] {
        let store = Store::new();
        let (mut options, _) = store.options(Scenario::Success)?;
        options.limits.deadline_ms = deadline_ms;
        options.limits.max_provider_calls = calls;
        let (provider, _) =
            ScriptedProvider::scenario(Scenario::Success, Duration::from_millis(delay_ms))?;
        let result = run(options, provider, |_| ())?;
        let record = inspect(&store.0, &result.run_id)?;
        assert_eq!(record.state, TaskState::Failed);
        assert!(
            record
                .outcome
                .ok_or("missing outcome")?
                .reason
                .contains(reason)
        );
        assert!(record.statistics.provider_attempts <= calls);
        assert!(record.pending_operation_ids.is_empty());
    }
    Ok(())
}

#[test]
fn oversized_output_is_bounded_and_cannot_pass() -> TestResult {
    let store = Store::new();
    let (mut options, _) = store.options(Scenario::Success)?;
    options.limits.max_output_bytes = 64;
    let provider = ScriptedProvider::new(
        vec![ScriptStep {
            expected_previous_result: None,
            reply: Ok(ModelReply::Final {
                text: "é".repeat(100),
            }),
        }],
        Duration::ZERO,
    )?;
    let result = run(options, provider, |_| ())?;
    let record = inspect(&store.0, &result.run_id)?;
    assert_eq!(record.state, TaskState::Failed);
    let preview = record
        .events
        .iter()
        .find_map(|event| match &event.data {
            EventData::OperationFinished {
                result: OperationResult::Error { preview, .. },
            } => preview.as_ref(),
            _ => None,
        })
        .ok_or("missing truncated output")?;
    assert!(preview.truncated);
    assert!(preview.text.len() <= 64);
    assert!(preview.original_bytes > 64);
    Ok(())
}

#[test]
fn output_limit_applies_to_tool_results() -> TestResult {
    let store = Store::new();
    let (mut options, provider) = store.options(Scenario::Success)?;
    options.limits.max_output_bytes = 90; // Model call fits; serialized tool evidence does not.
    let result = run(options, provider, |_| ())?;
    let record = inspect(&store.0, &result.run_id)?;
    assert_eq!(record.state, TaskState::Failed);
    assert_eq!(record.statistics.logical_tool_calls, 1);
    assert_eq!(record.statistics.tool_results, 0);
    assert!(
        record
            .outcome
            .ok_or("missing outcome")?
            .reason
            .contains("tool response")
    );
    Ok(())
}

#[test]
fn policy_rejects_unlisted_tools_and_schema_errors() {
    for name in ["shell", "read_file", "http.get", "../fixture.lookup"] {
        assert!(
            matches!(tool::execute(&ToolCall { name: name.into(), arguments: json!({"key": "greeting"}) }),
            ToolResult::Rejected { error } if error.kind == ErrorKind::PermissionDenied)
        );
    }
    for arguments in [
        json!({}),
        json!({"key": 1}),
        json!({"key": ""}),
        json!({"key": "a", "path": "/etc/passwd"}),
    ] {
        assert!(
            matches!(tool::execute(&ToolCall { name: TOOL_NAME.into(), arguments }),
            ToolResult::Rejected { error } if error.kind == ErrorKind::InvalidArguments)
        );
    }
}

#[test]
fn normalized_invalid_calls_emit_incidents_but_negative_results_do_not() -> TestResult {
    let first = ToolCall {
        name: TOOL_NAME.into(),
        arguments: serde_json::from_str(r#"{ "key": 7, "extra": true }"#)?,
    };
    let second = ToolCall {
        name: TOOL_NAME.into(),
        arguments: serde_json::from_str(r#"{"extra":true,"key":7}"#)?,
    };
    assert!(tool::equivalent(&first, &second));
    assert!(!tool::equivalent(
        &first,
        &ToolCall {
            name: TOOL_NAME.into(),
            arguments: json!({"key": 8})
        }
    ));
    let store = Store::new();
    let (options, _) = store.options(Scenario::Success)?;
    let rejected = tool::execute(&first);
    let provider = ScriptedProvider::new(
        vec![
            ScriptStep {
                expected_previous_result: None,
                reply: Ok(ModelReply::ToolCall(first)),
            },
            ScriptStep {
                expected_previous_result: Some(rejected.clone()),
                reply: Ok(ModelReply::ToolCall(second)),
            },
            ScriptStep {
                expected_previous_result: Some(rejected),
                reply: Ok(ModelReply::Final {
                    text: "done".into(),
                }),
            },
        ],
        Duration::ZERO,
    )?;
    let result = run(options, provider, |_| ())?;
    let repeated = inspect(&store.0, &result.run_id)?;
    assert_eq!(repeated.statistics.incidents, 1);
    assert_eq!(repeated.statistics.rejected_calls, 2);
    assert!(repeated.events.iter().any(|event| matches!(
        &event.data,
        EventData::Incident {
            repeat_count: 2,
            ..
        }
    )));
    let negative = Store::new().execute(Scenario::Negative)?;
    assert_eq!(negative.statistics.incidents, 0);
    assert_eq!(negative.statistics.rejected_calls, 0);
    assert_eq!(negative.statistics.expected_negative_results, 1);
    Ok(())
}

#[test]
fn restart_inspects_uncertain_boundaries_without_changing_records() -> TestResult {
    let source_store = Store::new();
    let source = source_store.execute(Scenario::Success)?;
    for after_result in [false, true] {
        let store = Store::new();
        fs::create_dir_all(&store.0)?;
        let path = record_path(&store.0, &source.run_id)?;
        let mut bytes = Vec::new();
        for event in &source.events {
            serde_json::to_writer(&mut bytes, event)?;
            bytes.push(b'\n');
            let boundary = if after_result {
                event.turn == 2
                    && matches!(
                        event.data,
                        EventData::OperationStarted {
                            request: OperationRequest::Provider { .. }
                        }
                    )
            } else {
                matches!(
                    event.data,
                    EventData::OperationStarted {
                        request: OperationRequest::Tool { .. }
                    }
                )
            };
            if boundary {
                break;
            }
        }
        fs::write(&path, &bytes)?;
        let record = inspect(&store.0, &source.run_id)?;
        assert!(record.incomplete);
        assert!(record.outcome.is_none());
        assert_eq!(record.pending_operation_ids.len(), 1);
        assert_eq!(record.statistics.logical_tool_calls, 1);
        assert_eq!(record.statistics.tool_results, u32::from(after_result));
        assert_eq!(fs::read(&path)?, bytes);
        OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(b"{\"partial\":")?;
        assert!(inspect(&store.0, &source.run_id)?.partial_tail);
    }
    Ok(())
}

#[test]
fn corruption_and_path_escape_are_rejected() -> TestResult {
    let store = Store::new();
    let record = store.execute(Scenario::Success)?;
    let path = record_path(&store.0, &record.run_id)?;
    let original = fs::read(&path)?;
    let mut events = record.events;
    events.first_mut().ok_or("no events")?.schema_version = 999;
    let mut corrupted = Vec::new();
    for event in &events {
        serde_json::to_writer(&mut corrupted, event)?;
        corrupted.push(b'\n');
    }
    fs::write(&path, &corrupted)?;
    assert!(inspect(&store.0, &record.run_id).is_err());
    fs::write(&path, b"not json\n")?;
    assert!(inspect(&store.0, &record.run_id).is_err());
    fs::write(&path, original)?;
    assert_eq!(
        inspect(&store.0, &record.run_id)?.state,
        TaskState::VerifiedSuccess
    );
    for id in ["../escape", "/tmp/escape", "", "a/b"] {
        assert!(record_path(&store.0, id).is_err());
    }
    Ok(())
}

#[test]
fn invalid_options_create_no_run() -> TestResult {
    let store = Store::new();
    let (mut options, provider) = store.options(Scenario::Success)?;
    options.limits.max_provider_calls = 33;
    assert!(run(options, provider, |_| ()).is_err());
    assert!(!store.0.exists());
    Ok(())
}

#[test]
fn storage_failure_prevents_provider_execution() -> TestResult {
    let store = Store::new();
    fs::write(&store.0, b"not a directory")?;
    let (options, provider) = store.options(Scenario::Success)?;
    let mut observed = 0;
    assert!(run(options, provider, |_| observed += 1).is_err());
    assert_eq!(observed, 0);
    fs::remove_file(&store.0)?;
    Ok(())
}

#[test]
fn oversized_error_diagnostics_remain_inspectable() -> TestResult {
    let store = Store::new();
    let (mut options, _) = store.options(Scenario::Success)?;
    options.limits.max_output_bytes = 8_192;
    let provider = ScriptedProvider::new(
        vec![ScriptStep {
            expected_previous_result: None,
            reply: Err(fishnet_core::protocol::OperationError {
                kind: ErrorKind::ProviderFailure,
                message: "\n".repeat(50_000),
            }),
        }],
        Duration::ZERO,
    )?;
    let result = run(options, provider, |_| ())?;
    let record = inspect(&store.0, &result.run_id)?;
    assert_eq!(record.state, TaskState::Failed);
    assert!(record.events.iter().any(|event| matches!(&event.data,
        EventData::OperationFinished { result: OperationResult::Error { preview: Some(preview), .. } }
            if preview.truncated && preview.original_bytes == 50_000 && preview.text.len() <= 8_192)));
    Ok(())
}

#[test]
fn repeated_invalid_calls_can_recover_without_changing_success_criteria() -> TestResult {
    let store = Store::new();
    let (options, _) = store.options(Scenario::Success)?;
    let invalid = ToolCall {
        name: TOOL_NAME.into(),
        arguments: json!({"key": 7}),
    };
    let valid = ToolCall {
        name: TOOL_NAME.into(),
        arguments: json!({"key": "greeting"}),
    };
    let rejected = tool::execute(&invalid);
    let found = tool::execute(&valid);
    let provider = ScriptedProvider::new(
        vec![
            ScriptStep {
                expected_previous_result: None,
                reply: Ok(ModelReply::ToolCall(invalid.clone())),
            },
            ScriptStep {
                expected_previous_result: Some(rejected.clone()),
                reply: Ok(ModelReply::ToolCall(invalid)),
            },
            ScriptStep {
                expected_previous_result: Some(rejected),
                reply: Ok(ModelReply::ToolCall(valid)),
            },
            ScriptStep {
                expected_previous_result: Some(found),
                reply: Ok(ModelReply::Final {
                    text: "done".into(),
                }),
            },
        ],
        Duration::ZERO,
    )?;
    let result = run(options, provider, |_| ())?;
    let record = inspect(&store.0, &result.run_id)?;
    assert_eq!(record.state, TaskState::VerifiedSuccess);
    assert_eq!(record.statistics.incidents, 1);
    assert_eq!(record.statistics.rejected_calls, 2);
    assert_eq!(record.statistics.logical_tool_calls, 3);
    Ok(())
}
