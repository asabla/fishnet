use std::error::Error;
use std::time::{Duration, Instant};

use fishnet_core::fixture::{Scenario, ScriptedProvider};
use fishnet_core::protocol::{BoundedText, ErrorKind, ModelReply, TaskState, ToolResult};
use fishnet_core::{Cancellation, tool};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn fixture_requires_a_matching_result_before_final_completion() -> TestResult {
    let (mut provider, _) = ScriptedProvider::scenario(Scenario::Success, Duration::ZERO)?;
    let control = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let ModelReply::ToolCall(call) = provider
        .complete(None, &control, deadline)
        .map_err(|error| error.message)?
    else {
        return Err("fixture did not request a tool".into());
    };
    let result = tool::execute(&call);
    assert!(matches!(&result, ToolResult::Found { value, .. } if value == "fishnet fixture ready"));
    assert!(matches!(
        provider.complete(Some(&result), &control, deadline),
        Ok(ModelReply::Final { .. })
    ));
    Ok(())
}

#[test]
fn cancelled_fixture_cannot_return_a_response() -> TestResult {
    let (mut provider, _) = ScriptedProvider::scenario(Scenario::Success, Duration::ZERO)?;
    let control = Cancellation::default();
    control.cancel();
    let result = provider.complete(None, &control, Instant::now() + Duration::from_secs(5));
    assert!(matches!(result, Err(error) if error.kind == ErrorKind::Cancelled));
    Ok(())
}

#[test]
fn terminal_states_cannot_reenter_execution() {
    for state in [
        TaskState::VerifiedSuccess,
        TaskState::UnverifiedCompletion,
        TaskState::Failed,
        TaskState::Cancelled,
    ] {
        assert!(state.is_terminal());
        assert!(!state.permits(TaskState::Running));
        assert!(!state.permits(TaskState::Queued));
    }
    assert!(!TaskState::Queued.permits(TaskState::VerifiedSuccess));
    assert!(!TaskState::Running.permits(TaskState::Cancelled));
}

#[test]
fn diagnostic_prefixes_preserve_utf8_and_report_truncation() {
    let text = BoundedText::new("éé", 3);
    assert_eq!(text.text, "é");
    assert!(text.truncated);
    assert_eq!(text.original_bytes, 4);
}
