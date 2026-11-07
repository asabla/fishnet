use serde::Deserialize;

use crate::protocol::{ErrorKind, FIXTURE_REVISION, OperationError, ToolCall, ToolResult};

pub const TOOL_NAME: &str = "fixture.lookup";
pub const GREETING: &str = "fishnet fixture ready";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LookupArguments {
    key: String,
}

/// The only M1 capability. No paths, processes, network, writes, or dynamic tools.
#[must_use]
pub fn execute(call: &ToolCall) -> ToolResult {
    if call.name != TOOL_NAME {
        return rejected(
            ErrorKind::PermissionDenied,
            "tool is not permitted by fixture-read-only-v1",
        );
    }
    let args: LookupArguments = match serde_json::from_value(call.arguments.clone()) {
        Ok(args) => args,
        Err(_) => {
            return rejected(
                ErrorKind::InvalidArguments,
                "expected exactly one string field: key",
            );
        }
    };
    if args.key.is_empty() || args.key.len() > 64 {
        return rejected(
            ErrorKind::InvalidArguments,
            "key must contain 1..=64 UTF-8 bytes",
        );
    }
    if args.key == "greeting" {
        ToolResult::Found {
            key: args.key,
            value: GREETING.into(),
            revision: FIXTURE_REVISION.into(),
        }
    } else {
        ToolResult::ExpectedNegative {
            key: args.key,
            revision: FIXTURE_REVISION.into(),
        }
    }
}

fn rejected(kind: ErrorKind, message: &str) -> ToolResult {
    ToolResult::Rejected {
        error: OperationError {
            kind,
            message: message.into(),
        },
    }
}

/// Compare parsed JSON values: object key order and whitespace do not change identity.
/// The immutable catalog, tool version, and policy form the relevant state in M1.
#[must_use]
pub fn equivalent(a: &ToolCall, b: &ToolCall) -> bool {
    a == b
}
