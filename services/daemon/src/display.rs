//! Human-readable views of service evidence. These functions do not infer outcomes.
use crate::contract::Phase;
use fishnet_core::TaskState;
use fishnet_core::protocol::{
    EventData, ModelReply, OperationRequest, OperationResult, ToolResult,
};
use std::fmt::Write as _;

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
            Phase::Failed => "FAILED",
            Phase::Finished | Phase::Interrupted => "UNKNOWN / INTERRUPTED",
        },
    }
}

pub fn describe(data: &EventData) -> String {
    let text = match data {
        EventData::RunCreated { contract, .. } => {
            format!("Task started: {}", safe(&contract.objective, 1024))
        }
        EventData::StateChanged { to, .. } => {
            format!("State: {}", status(Phase::Executing, Some(*to), false))
        }
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
            format!(
                "Outcome: {}: {}",
                status(Phase::Finished, Some(outcome.state), false),
                outcome.reason
            )
        }
    };
    text.lines()
        .map(|line| safe(line, 8192))
        .collect::<Vec<_>>()
        .join("\n")
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

#[must_use]
pub fn review(grant: &crate::contract::ApprovalView) -> String {
    if grant.conversation {
        return format!(
            "{}\n\nDeadline: {} ms; response: {} bytes\nContext: at most 16 turns / 32 KiB; no silent truncation.\nGrant fingerprint: {}",
            approval_summary(grant),
            grant.limits.deadline_ms,
            grant.limits.max_output_bytes,
            grant.fingerprint
        );
    }
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
        "\nGrant fingerprint: {}\n\nApproval permits source and command output to be sent to the configured provider.\nCommands use the host OS permissions.\nQuality checks establish the listed checks, not every claim about the requested behavior. Review the source diff and add protected behavioral checks when needed.",
        grant.fingerprint
    );
    text
}

/// Present the scope of consent without exposing transport or command syntax.
#[must_use]
pub fn approval_summary(grant: &crate::contract::ApprovalView) -> String {
    let identity = format!(
        "{} · {} · {}",
        safe(&grant.provider, 512),
        safe(&grant.account, 64),
        safe(grant.model.as_deref().unwrap_or("host default"), 128)
    );
    if grant.conversation {
        return format!(
            "{identity}\n\nSend messages in this conversation to this provider.\nOnly conversation messages are sent.\nNo automatic file reads or commands are permitted.\n\nConsent also covers later messages in this conversation.\nResponses are unverified. /new starts a separate conversation."
        );
    }
    format!(
        "{identity}\nWorkspace: {}\n\nFiles to change: {}\nChecks: {}\n\nSource files and approved command output may be sent to this provider.\nCommands use the host OS permissions. Review full details with F2.",
        safe(&grant.root.display().to_string(), 512),
        if grant.allowed_writes.is_empty() {
            "none".into()
        } else {
            grant
                .allowed_writes
                .iter()
                .map(|s| safe(s, 512))
                .collect::<Vec<_>>()
                .join(", ")
        },
        grant
            .checks
            .iter()
            .map(|s| safe(s, 64))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
