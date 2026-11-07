use std::collections::VecDeque;
use std::fmt::Write as _;

use fishnet_core::{
    Event,
    protocol::{EventData, ModelReply, OperationRequest, OperationResult, ToolResult},
};
use fishnet_service::display::{describe, safe};

pub const MAX_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    User,
    Assistant,
    Tool,
    Check,
    Error,
    Outcome,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: EntryKind,
    pub text: String,
    pub operation_id: Option<String>,
}

/// Project durable observations into display content; retain raw events separately.
pub(super) fn append(entries: &mut VecDeque<Entry>, event: &Event) {
    let Some((kind, text)) = content(&event.data) else {
        return;
    };
    let mut text = bounded(&text);
    if matches!(event.data, EventData::OperationFinished { .. }) {
        if let Some(duration) = event.duration_us {
            let _ = write!(text, "  ({} ms)", duration / 1000);
        }
        if let Some(previous) = entries
            .iter_mut()
            .rev()
            .find(|entry| event.operation_id.is_some() && entry.operation_id == event.operation_id)
        {
            previous.text.push('\n');
            previous.text.push_str(&text);
            previous.text = bounded(&previous.text);
            if kind == EntryKind::Error {
                previous.kind = kind;
            }
            return;
        }
    }
    entries.push_back(Entry {
        kind,
        text,
        operation_id: event.operation_id.clone(),
    });
    while entries.len() > MAX_ENTRIES {
        entries.pop_front();
    }
}

fn content(data: &EventData) -> Option<(EntryKind, String)> {
    let content = match data {
        EventData::RunCreated { contract, .. } => (EntryKind::User, contract.objective.clone()),
        EventData::OperationStarted {
            request: OperationRequest::Tool { call, .. },
        } => {
            let target = call
                .arguments
                .get("path")
                .or_else(|| call.arguments.get("command_id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            (EntryKind::Tool, format!("{} {target}", call.name))
        }
        EventData::OperationStarted {
            request: OperationRequest::Check { checker, .. },
        } => (EntryKind::Check, format!("Independent check · {checker}")),
        EventData::OperationFinished { result } => match result {
            OperationResult::Model {
                reply: ModelReply::Final { text },
                ..
            } => (EntryKind::Assistant, text.clone()),
            OperationResult::Tool {
                result: ToolResult::Rejected { .. },
            }
            | OperationResult::Error { .. } => (EntryKind::Error, preview(&describe(data))),
            OperationResult::Tool { .. } => (EntryKind::Tool, preview(&describe(data))),
            OperationResult::Check { status, .. } => (EntryKind::Check, format!("{status:?}")),
            OperationResult::Model {
                reply: ModelReply::ToolCall(_),
                ..
            } => return None,
        },
        EventData::TaskFinished { .. } => (EntryKind::Outcome, describe(data)),
        EventData::Incident { .. } => (EntryKind::Error, describe(data)),
        EventData::StateChanged { .. } | EventData::OperationStarted { .. } => return None,
    };
    Some(content)
}

fn preview(text: &str) -> String {
    let mut lines = text.lines();
    let mut preview = lines.by_ref().take(4).collect::<Vec<_>>().join("\n");
    if lines.next().is_some() {
        preview.push_str("\n… /details for recent output; full journal on disk");
    }
    preview
}

fn bounded(text: &str) -> String {
    let mut result = text
        .lines()
        .take(48)
        .map(|line| safe(line, 4096))
        .collect::<Vec<_>>()
        .join("\n");
    if result.len() > 4096 || text.lines().count() > 48 || result.len() < text.len() {
        result = fishnet_core::protocol::BoundedText::new(&result, 4096).text;
        result.push_str("\n… [preview truncated; full journal on disk]");
    }
    result
}
