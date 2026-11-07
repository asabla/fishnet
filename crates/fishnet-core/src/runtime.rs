use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::control::Cancellation;
use crate::fixture::{PROVIDER_VERSION, ScriptedProvider};
use crate::protocol::{
    BoundedText, CheckStatus, ErrorKind, Event, EventData, FIXTURE_REVISION, Limits, ModelReply,
    OperationError, OperationRequest, OperationResult, Outcome, POLICY_VERSION, SCHEMA_VERSION,
    TaskContract, TaskState, ToolCall, ToolResult,
};
use crate::storage::{Journal, invalid};
use crate::tool;

#[derive(Debug)]
pub struct RunOptions {
    pub store: PathBuf,
    pub limits: Limits,
    pub contract: TaskContract,
    pub cancellation: Cancellation,
}

#[derive(Debug)]
pub struct RunSummary {
    pub run_id: String,
    pub outcome: Outcome,
}

struct Operation {
    id: String,
    parent: Option<String>,
    started: Instant,
}

#[derive(Debug)]
enum ExecutionError {
    Storage(io::Error),
    Halt(OperationError),
}

impl From<io::Error> for ExecutionError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<OperationError> for ExecutionError {
    fn from(error: OperationError) -> Self {
        Self::Halt(error)
    }
}

struct Executor<'a, F> {
    journal: Journal,
    options: RunOptions,
    started: Instant,
    deadline: Instant,
    state: TaskState,
    sequence: u64,
    turn: u32,
    operations: u32,
    observe: &'a mut F,
}

/// Persist each observation before publishing it. Storage errors stop all execution.
/// Observer callbacks must return promptly; the CLI uses them only for JSON output.
///
/// # Errors
/// Returns an error for invalid options or a failed durable journal write.
pub fn run<F: FnMut(&Event)>(
    options: RunOptions,
    mut provider: ScriptedProvider,
    mut observe: F,
) -> io::Result<RunSummary> {
    options.limits.validate().map_err(invalid)?;
    validate_contract(&options.contract)?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(options.limits.deadline_ms);
    let journal = Journal::create(&options.store)?;
    let mut executor = Executor {
        journal,
        options,
        started,
        deadline,
        state: TaskState::Queued,
        sequence: 0,
        turn: 0,
        operations: 0,
        observe: &mut observe,
    };
    executor.emit(
        EventData::RunCreated {
            contract: executor.options.contract.clone(),
            limits: executor.options.limits.clone(),
            provider_version: PROVIDER_VERSION.into(),
        },
        None,
        None,
    )?;
    if let Err(error) = executor.check_control() {
        return executor.stop(&error);
    }
    executor.transition(TaskState::Running)?;
    match executor.execute(&mut provider) {
        Ok(summary) => Ok(summary),
        Err(ExecutionError::Storage(error)) => Err(error),
        Err(ExecutionError::Halt(error)) => executor.stop(&error),
    }
}

fn validate_contract(contract: &TaskContract) -> io::Result<()> {
    if contract.policy_version != POLICY_VERSION
        || contract.fixture_revision != FIXTURE_REVISION
        || contract.objective.len() > 512
        || contract.expected_key.is_empty()
        || contract.expected_key.len() > 64
        || contract
            .expected_value
            .as_ref()
            .is_some_and(|value| value.len() > 512)
    {
        return Err(invalid("unsupported or oversized fixture task contract"));
    }
    Ok(())
}

impl<F: FnMut(&Event)> Executor<'_, F> {
    fn execute(&mut self, provider: &mut ScriptedProvider) -> Result<RunSummary, ExecutionError> {
        let mut previous: Option<(String, ToolResult)> = None;
        let mut evidence = Vec::new();
        let mut invalid_calls: Vec<(ToolCall, String, u32)> = Vec::new();

        for turn in 1..=self.options.limits.max_provider_calls {
            self.turn = turn;
            self.check_control()?;
            let operation = self.start_operation(
                OperationRequest::Provider {
                    provider: PROVIDER_VERSION.into(),
                    previous_tool_operation_id: previous.as_ref().map(|(id, _)| id.clone()),
                },
                None,
            )?;
            let reply = self.request_model(
                provider,
                previous.as_ref().map(|(_, result)| result),
                &operation,
            )?;
            self.check_control()?;

            match reply {
                ModelReply::ToolCall(call) => {
                    let tool_operation = self.start_operation(
                        OperationRequest::Tool {
                            call: call.clone(),
                            relevant_state: relevant_state(),
                        },
                        Some(operation.id),
                    )?;
                    let result = self.execute_tool(&call, &tool_operation)?;
                    if matches!(&result, ToolResult::Rejected { .. }) {
                        self.record_invalid_call(&call, &tool_operation, &mut invalid_calls)?;
                    }
                    evidence.push((tool_operation.id.clone(), result.clone()));
                    previous = Some((tool_operation.id, result));
                }
                ModelReply::Final { text } => return self.verify(&evidence, &text, operation.id),
            }
        }
        self.check_control()?;
        Err(OperationError {
            kind: ErrorKind::CallLimit,
            message: "provider call budget exhausted".into(),
        }
        .into())
    }

    fn request_model(
        &mut self,
        provider: &mut ScriptedProvider,
        previous: Option<&ToolResult>,
        operation: &Operation,
    ) -> Result<ModelReply, ExecutionError> {
        let reply = match provider.complete(previous, &self.options.cancellation, self.deadline) {
            Ok(reply) => reply,
            Err(error) => {
                self.finish_error(operation, &error, None)?;
                return Err(error.into());
            }
        };
        let serialized = serde_json::to_string(&reply).map_err(io::Error::other)?;
        if serialized.len() > self.options.limits.max_output_bytes {
            let error = OperationError {
                kind: ErrorKind::OutputLimit,
                message: "model response exceeds output limit".into(),
            };
            self.finish_error(operation, &error, Some(&serialized))?;
            return Err(error.into());
        }
        self.finish_operation(
            operation,
            OperationResult::Model {
                reply: reply.clone(),
            },
        )?;
        Ok(reply)
    }

    fn execute_tool(
        &mut self,
        call: &ToolCall,
        operation: &Operation,
    ) -> Result<ToolResult, ExecutionError> {
        // The persisted start is the execution boundary. Check cancellation again.
        if let Err(error) = self.check_control() {
            self.finish_error(operation, &error, None)?;
            return Err(error.into());
        }
        let result = tool::execute(call);
        if serde_json::to_vec(&result).map_err(io::Error::other)?.len()
            > self.options.limits.max_output_bytes
        {
            let error = OperationError {
                kind: ErrorKind::OutputLimit,
                message: "tool response exceeds output limit".into(),
            };
            self.finish_error(operation, &error, None)?;
            return Err(error.into());
        }
        self.finish_operation(
            operation,
            OperationResult::Tool {
                result: result.clone(),
            },
        )?;
        Ok(result)
    }

    fn verify(
        &mut self,
        evidence: &[(String, ToolResult)],
        text: &str,
        parent: String,
    ) -> Result<RunSummary, ExecutionError> {
        let operation = self.start_operation(
            OperationRequest::Check {
                checker: "fixture-outcome-v1".into(),
            },
            Some(parent),
        )?;
        if let Err(error) = self.check_control() {
            self.finish_error(&operation, &error, None)?;
            return Err(error.into());
        }
        let (status, source) = check_fixture(&self.options.contract, evidence);
        self.finish_operation(
            &operation,
            OperationResult::Check {
                status,
                evidence_operation_id: source,
            },
        )?;
        self.check_control()?;
        let (state, reason) = match status {
            CheckStatus::Passed => (
                TaskState::VerifiedSuccess,
                "independent fixture check passed",
            ),
            CheckStatus::Failed => (TaskState::Failed, "independent fixture check failed"),
            CheckStatus::Unavailable => (
                TaskState::UnverifiedCompletion,
                "model completed without sufficient independent evidence",
            ),
        };
        self.finish(Outcome {
            state,
            reason: reason.into(),
            evidence_operation_id: Some(operation.id),
            final_text: Some(BoundedText::new(text, self.options.limits.max_output_bytes)),
        })
        .map_err(Into::into)
    }

    fn record_invalid_call(
        &mut self,
        call: &ToolCall,
        operation: &Operation,
        previous: &mut Vec<(ToolCall, String, u32)>,
    ) -> io::Result<()> {
        if let Some((_, first, count)) = previous
            .iter_mut()
            .find(|(prior, _, _)| tool::equivalent(prior, call))
        {
            *count += 1;
            self.emit(
                EventData::Incident {
                    code: "equivalent_repeated_invalid_call".into(),
                    first_operation_id: first.clone(),
                    repeat_count: *count,
                    relevant_state: relevant_state(),
                },
                Some(operation),
                None,
            )?;
        } else {
            previous.push((call.clone(), operation.id.clone(), 1));
        }
        Ok(())
    }

    fn check_control(&self) -> Result<(), OperationError> {
        self.options.cancellation.check(self.deadline)
    }

    fn stop(&mut self, error: &OperationError) -> io::Result<RunSummary> {
        let state = if error.kind == ErrorKind::Cancelled {
            self.transition(TaskState::Cancelling)?;
            TaskState::Cancelled
        } else {
            TaskState::Failed
        };
        // A zero deadline can fail before Running. Preserve a valid transition path.
        if self.state == TaskState::Queued {
            self.transition(TaskState::Running)?;
        }
        self.finish(Outcome {
            state,
            reason: format!(
                "{:?}: {}",
                error.kind,
                BoundedText::new(&error.message, self.options.limits.max_output_bytes).text
            ),
            evidence_operation_id: None,
            final_text: None,
        })
    }

    fn transition(&mut self, to: TaskState) -> io::Result<()> {
        if !self.state.permits(to) {
            return Err(invalid("runtime rejected state transition"));
        }
        self.emit(
            EventData::StateChanged {
                from: self.state,
                to,
            },
            None,
            None,
        )?;
        self.state = to;
        Ok(())
    }

    fn finish(&mut self, outcome: Outcome) -> io::Result<RunSummary> {
        if !outcome.state.is_terminal() || !self.state.permits(outcome.state) {
            return Err(invalid("runtime rejected terminal outcome"));
        }
        self.emit(
            EventData::TaskFinished {
                outcome: outcome.clone(),
            },
            None,
            Some(micros(self.started.elapsed())),
        )?;
        self.state = outcome.state;
        Ok(RunSummary {
            run_id: self.journal.run_id.clone(),
            outcome,
        })
    }

    fn start_operation(
        &mut self,
        request: OperationRequest,
        parent: Option<String>,
    ) -> io::Result<Operation> {
        self.operations += 1;
        let operation = Operation {
            id: format!("{}-op-{}", self.journal.run_id, self.operations),
            parent,
            started: Instant::now(),
        };
        self.emit(
            EventData::OperationStarted { request },
            Some(&operation),
            None,
        )?;
        Ok(operation)
    }

    fn finish_operation(
        &mut self,
        operation: &Operation,
        result: OperationResult,
    ) -> io::Result<()> {
        self.emit(
            EventData::OperationFinished { result },
            Some(operation),
            Some(micros(operation.started.elapsed())),
        )
    }

    fn finish_error(
        &mut self,
        operation: &Operation,
        error: &OperationError,
        preview: Option<&str>,
    ) -> io::Result<()> {
        let limit = self.options.limits.max_output_bytes;
        let bounded_error = BoundedText::new(&error.message, limit);
        let preview = preview.or_else(|| bounded_error.truncated.then_some(error.message.as_str()));
        let message = if bounded_error.truncated {
            "error message truncated".into()
        } else {
            bounded_error.text
        };
        self.finish_operation(
            operation,
            OperationResult::Error {
                error: OperationError {
                    kind: error.kind,
                    message,
                },
                preview: preview.map(|text| BoundedText::new(text, limit)),
            },
        )
    }

    fn emit(
        &mut self,
        data: EventData,
        operation: Option<&Operation>,
        duration_us: Option<u64>,
    ) -> io::Result<()> {
        self.sequence += 1;
        let event = Event {
            schema_version: SCHEMA_VERSION,
            session_id: self.journal.run_id.clone(),
            task_id: self.journal.run_id.clone(),
            trace_id: self.journal.run_id.clone(),
            sequence: self.sequence,
            timestamp_unix_ms: u64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(io::Error::other)?
                    .as_millis(),
            )
            .map_err(io::Error::other)?,
            elapsed_us: micros(self.started.elapsed()),
            turn: self.turn,
            operation_id: operation.map(|op| op.id.clone()),
            parent_operation_id: operation.and_then(|op| op.parent.clone()),
            attempt: operation.map(|_| 1),
            duration_us,
            data,
        };
        self.journal.append(&event)?;
        (self.observe)(&event);
        Ok(())
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn relevant_state() -> String {
    format!("{FIXTURE_REVISION}/{POLICY_VERSION}/fixture.lookup-v1")
}

/// Check recorded tool evidence against the protected task contract. Do not call the tool.
fn check_fixture(
    contract: &TaskContract,
    evidence: &[(String, ToolResult)],
) -> (CheckStatus, Option<String>) {
    for (id, result) in evidence.iter().rev() {
        let matches = match result {
            ToolResult::Found {
                key,
                value,
                revision,
            } if key == &contract.expected_key && revision == &contract.fixture_revision => {
                Some(contract.expected_value.as_ref() == Some(value))
            }
            ToolResult::ExpectedNegative { key, revision }
                if key == &contract.expected_key && revision == &contract.fixture_revision =>
            {
                Some(contract.expected_value.is_none())
            }
            _ => None,
        };
        if let Some(passed) = matches {
            return (
                if passed {
                    CheckStatus::Passed
                } else {
                    CheckStatus::Failed
                },
                Some(id.clone()),
            );
        }
    }
    (CheckStatus::Unavailable, None)
}
