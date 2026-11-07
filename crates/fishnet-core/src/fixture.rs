use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::control::Cancellation;
use crate::protocol::{
    ErrorKind, FIXTURE_REVISION, ModelReply, OperationError, POLICY_VERSION, TaskContract,
    ToolCall, ToolResult,
};
use crate::tool::TOOL_NAME;

// This expectation is separate from the tool implementation.
const EXPECTED_GREETING: &str = "fishnet fixture ready";

pub const PROVIDER_VERSION: &str = "scripted-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scenario {
    Success,
    Unverified,
    CheckFailure,
    RepeatedInvalid,
    Negative,
    ProviderFailure,
}

impl Scenario {
    pub const NAMES: &str =
        "success, unverified, check-failure, repeated-invalid, negative, provider-failure";

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "success" => Self::Success,
            "unverified" => Self::Unverified,
            "check-failure" => Self::CheckFailure,
            "repeated-invalid" => Self::RepeatedInvalid,
            "negative" => Self::Negative,
            "provider-failure" => Self::ProviderFailure,
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub struct ScriptStep {
    pub expected_previous_result: Option<ToolResult>,
    pub reply: Result<ModelReply, OperationError>,
}

/// Scripted responses plus explicit result expectations exercise the owned loop.
/// This is a fixture, not a strategy or a live-provider evaluation.
#[derive(Debug)]
pub struct ScriptedProvider {
    steps: VecDeque<ScriptStep>,
    delay: Duration,
}

impl ScriptedProvider {
    /// # Errors
    /// Rejects scripts or delays above the fixed fixture bounds.
    pub fn new(steps: Vec<ScriptStep>, delay: Duration) -> Result<Self, &'static str> {
        if steps.len() > 32 || delay > Duration::from_secs(300) {
            return Err("fixture script exceeds 32 steps or 300 second delay");
        }
        Ok(Self {
            steps: steps.into(),
            delay,
        })
    }

    /// # Errors
    /// Rejects a delay above the fixed fixture bound.
    pub fn scenario(
        scenario: Scenario,
        delay: Duration,
    ) -> Result<(Self, TaskContract), &'static str> {
        let mut contract = TaskContract {
            conversation: None,
            objective: "Read greeting from the fixed catalog and verify the recorded result".into(),
            expected_key: "greeting".into(),
            expected_value: Some(EXPECTED_GREETING.into()),
            policy_version: POLICY_VERSION.into(),
            fixture_revision: FIXTURE_REVISION.into(),
            workspace: None,
        };
        let mut steps = Vec::new();
        let final_reply = ModelReply::Final {
            text: "Fixture complete.".into(),
        };
        match scenario {
            Scenario::Unverified => steps.push(ScriptStep {
                expected_previous_result: None,
                reply: Ok(final_reply),
            }),
            Scenario::ProviderFailure => steps.push(ScriptStep {
                expected_previous_result: None,
                reply: Err(OperationError {
                    kind: ErrorKind::ProviderFailure,
                    message: "scripted provider failure".into(),
                }),
            }),
            Scenario::RepeatedInvalid => steps = repeated_invalid_steps(final_reply),
            Scenario::Success | Scenario::CheckFailure | Scenario::Negative => {
                let key = if scenario == Scenario::Negative {
                    "absent"
                } else {
                    "greeting"
                };
                if scenario == Scenario::Negative {
                    contract.objective = "Confirm that absent is not in the fixed catalog".into();
                    contract.expected_key = key.into();
                    contract.expected_value = None;
                }
                let call = ToolCall {
                    name: TOOL_NAME.into(),
                    arguments: json!({"key": key}),
                };
                let result = if scenario == Scenario::Negative {
                    ToolResult::ExpectedNegative {
                        key: key.into(),
                        revision: FIXTURE_REVISION.into(),
                    }
                } else {
                    ToolResult::Found {
                        key: key.into(),
                        value: EXPECTED_GREETING.into(),
                        revision: FIXTURE_REVISION.into(),
                    }
                };
                if scenario == Scenario::CheckFailure {
                    contract.expected_value = Some("deliberate mismatch".into());
                }
                steps.push(ScriptStep {
                    expected_previous_result: None,
                    reply: Ok(ModelReply::ToolCall(call)),
                });
                steps.push(ScriptStep {
                    expected_previous_result: Some(result),
                    reply: Ok(final_reply),
                });
            }
        }
        Ok((Self::new(steps, delay)?, contract))
    }

    /// Consume one scripted response after checking the preceding tool result.
    ///
    /// # Errors
    /// Returns cancellation, deadline, script exhaustion, result mismatch, or a scripted error.
    pub fn complete(
        &mut self,
        previous: Option<&ToolResult>,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> Result<ModelReply, OperationError> {
        cancellation.wait(self.delay, deadline)?;
        self.take_reply(previous)
    }

    pub(crate) async fn complete_async(
        &mut self,
        previous: Option<&ToolResult>,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> Result<ModelReply, OperationError> {
        cancellation
            .interrupt(deadline, tokio::time::sleep(self.delay))
            .await?;
        self.take_reply(previous)
    }

    fn take_reply(&mut self, previous: Option<&ToolResult>) -> Result<ModelReply, OperationError> {
        let step = self.steps.pop_front().ok_or_else(|| OperationError {
            kind: ErrorKind::ProviderFailure,
            message: "fixture script exhausted".into(),
        })?;
        if step.expected_previous_result.as_ref() != previous {
            return Err(OperationError {
                kind: ErrorKind::ProviderFailure,
                message: "fixture did not receive the expected tool result".into(),
            });
        }
        step.reply
    }
}

fn repeated_invalid_steps(final_reply: ModelReply) -> Vec<ScriptStep> {
    let call = ToolCall {
        name: TOOL_NAME.into(),
        arguments: json!({"key": 7, "extra": true}),
    };
    let result = ToolResult::Rejected {
        error: OperationError {
            kind: ErrorKind::InvalidArguments,
            message: "expected exactly one string field: key".into(),
        },
    };
    vec![
        ScriptStep {
            expected_previous_result: None,
            reply: Ok(ModelReply::ToolCall(call.clone())),
        },
        ScriptStep {
            expected_previous_result: Some(result.clone()),
            reply: Ok(ModelReply::ToolCall(call)),
        },
        ScriptStep {
            expected_previous_result: Some(result),
            reply: Ok(final_reply),
        },
    ]
}
