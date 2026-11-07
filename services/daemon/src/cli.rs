//! Command-line transport for the common service API.
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use fishnet_core::{Cancellation, Limits, TaskState};

use crate::{
    client::Client,
    contract::{Approval, Phase, Submission, TaskRequest, TaskView},
};

type CliResult<T> = Result<T, Box<dyn std::error::Error>>;
struct Options {
    command: String,
    id: Option<String>,
    connection: PathBuf,
    request_id: Option<String>,
    task: Option<String>,
    scenario: String,
    delay_ms: u64,
    deadline_ms: u64,
    model: Option<String>,
    account: String,
    after: u64,
    revision: u64,
}

/// Execute a thin service client command. Provider access and policy remain on the host.
///
/// # Errors
/// Returns input, connection, authorization, protocol, or output errors.
pub fn execute(args: impl Iterator<Item = String>) -> CliResult<u8> {
    let options = parse(args)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(command(options))
}

fn parse(mut args: impl Iterator<Item = String>) -> CliResult<Options> {
    let command = args
        .next()
        .ok_or("task requires list, submit, status, watch, cancel, approve, or reject")?;
    let id = if matches!(
        command.as_str(),
        "status" | "watch" | "cancel" | "approve" | "reject"
    ) {
        Some(args.next().ok_or("task command requires a task ID")?)
    } else {
        None
    };
    let mut options = Options {
        command,
        id,
        connection: ".fishnet/service/connection.json".into(),
        request_id: None,
        task: None,
        scenario: "success".into(),
        delay_ms: 0,
        deadline_ms: 5000,
        model: None,
        account: "default".into(),
        after: 0,
        revision: 1,
    };
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--connection" => options.connection = value.into(),
            "--request-id" => options.request_id = Some(value),
            "--task" => options.task = Some(value),
            "--scenario" => options.scenario = value,
            "--delay-ms" => options.delay_ms = value.parse()?,
            "--deadline-ms" => options.deadline_ms = value.parse()?,
            "--model" => options.model = Some(value),
            "--account" => options.account = value,
            "--after" => options.after = value.parse()?,
            "--revision" => options.revision = value.parse()?,
            _ => return Err(format!("unknown task option: {option}").into()),
        }
    }
    Ok(options)
}

async fn command(options: Options) -> CliResult<u8> {
    let client = Client::connect(&options.connection)?;
    let id = options.id.as_deref().unwrap_or_default();
    let view = match options.command.as_str() {
        "list" => {
            print(&client.list().await?)?;
            return Ok(0);
        }
        "submit" => {
            let request_id = options.request_id.unwrap_or(crate::new_id()?);
            let task = if let Some(task_path) = options.task {
                TaskRequest::Code {
                    task_path,
                    model: options.model,
                    account: options.account,
                }
            } else {
                TaskRequest::Fixture {
                    scenario: options.scenario,
                    delay_ms: options.delay_ms,
                    limits: Limits {
                        deadline_ms: options.deadline_ms,
                        ..Limits::default()
                    },
                }
            };
            client.submit(Submission { request_id, task }).await?
        }
        "status" => client.snapshot(id).await?,
        "cancel" => client.cancel(id).await?,
        "approve" | "reject" => {
            client
                .approve(
                    id,
                    Approval {
                        expected_revision: options.revision,
                        approve: options.command == "approve",
                    },
                )
                .await?
        }
        "watch" => return watch(&options).await,
        _ => return Err("unknown task command".into()),
    };
    print(&view)?;
    Ok(0)
}

fn print(value: &impl serde::Serialize) -> CliResult<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

async fn watch(options: &Options) -> CliResult<u8> {
    let id = options.id.as_deref().ok_or("watch requires a task ID")?;
    let detach = Cancellation::default();
    let signal = detach.clone();
    ctrlc::set_handler(move || signal.cancel())?;
    let mut after = options.after;
    let mut failures = 0;
    while !detach.is_cancelled() {
        let client = Client::connect(&options.connection)?;
        match client.events(id, after, 500).await {
            Ok(page) => {
                failures = 0;
                if page.gap {
                    print(&page)?;
                    after = page.retained_from.saturating_sub(1);
                    continue;
                }
                for event in &page.events {
                    print(event)?;
                }
                after = page.cursor;
                if page.snapshot.phase.terminal() && after >= page.snapshot.cursor {
                    print(&page.snapshot)?;
                    return Ok(exit_code(&page.snapshot));
                }
            }
            Err(error) => {
                print(&error)?;
                failures += 1;
                if failures >= 5 || error.code != "disconnected" {
                    return Err(error.into());
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    Ok(130)
}

/// Map a terminal service snapshot to a task outcome exit code.
#[must_use]
pub fn exit_code(view: &TaskView) -> u8 {
    if let Some(run) = &view.run {
        return match run.state {
            TaskState::VerifiedSuccess => 0,
            TaskState::UnverifiedCompletion => 3,
            TaskState::Cancelled => 130,
            _ => 2,
        };
    }
    if view.phase == Phase::Cancelled {
        130
    } else {
        2
    }
}
