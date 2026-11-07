//! Command-line transport for the common service API.
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use fishnet_core::{Cancellation, Limits, TaskState};

use crate::{
    client::Client,
    contract::{Approval, Phase, Submission, TaskRequest, TaskView},
};

const HELP: &str = "Task commands:
  fishnet task list                         List tasks
  fishnet task submit --task tasks/FILE.json Prepare a reviewed task file
  fishnet task submit                       Run an offline fixture
  fishnet task status ID                    Show outcome and pending grant
  fishnet task watch ID                     Follow durable output; Ctrl-C detaches
  fishnet task cancel ID                    Cancel accepted work
  fishnet task approve ID --revision N      Approve the displayed grant revision
  fishnet task reject ID --revision N       Reject that revision

IDs accept a unique prefix. --json preserves machine-readable output.
Options: --connection PATH, --request-id ID, --model NAME, --account LABEL,
         --scenario NAME, --delay-ms N, --deadline-ms N, --after SEQUENCE.
Use fishnet for the interactive workflow. Submission alone does not mean success.";

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}
async fn resolve(client: &Client, id: &str) -> CliResult<String> {
    crate::contract::validate_key(id)?;
    if id.len() == 64 {
        return Ok(id.into());
    }
    let matches = client
        .list()
        .await?
        .into_iter()
        .filter(|task| task.task_id.starts_with(id))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [task] => Ok(task.task_id.clone()),
        [] => Err("no matching task; use fishnet tasks".into()),
        _ => Err("task prefix is ambiguous; use more characters".into()),
    }
}
fn show(view: &TaskView, json: bool) -> CliResult<()> {
    if json {
        return print(view);
    }
    println!(
        "{}\nTask: {}",
        crate::display::status(
            view.phase,
            view.run.as_ref().map(|r| r.state),
            view.run.as_ref().is_some_and(|r| r.incomplete)
        ),
        view.task_id
    );
    if let Some(message) = &view.message {
        println!("{}", crate::display::safe(message, 2048));
    }
    if let Some(run) = &view.run {
        if let Some(outcome) = &run.outcome {
            println!("{}", crate::display::safe(&outcome.reason, 2048));
        }
        println!(
            "Model requests: {}  Tool calls: {}  Processes: {}",
            run.statistics.provider_attempts,
            run.statistics.logical_tool_calls,
            run.statistics.process_attempts
        );
    }
    if let Some(grant) = &view.approval {
        println!(
            "\n{}\nApprove: fishnet task approve {} --revision {}",
            crate::display::review(grant),
            short(&view.task_id),
            grant.revision
        );
    }
    Ok(())
}

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
    json: bool,
}

/// Execute a thin service client command. Provider access and policy remain on the host.
///
/// # Errors
/// Returns input, connection, authorization, protocol, or output errors.
pub fn execute(args: impl Iterator<Item = String>) -> CliResult<u8> {
    let args = args.collect::<Vec<_>>();
    if args.is_empty()
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{HELP}");
        return Ok(0);
    }
    let options = parse(args.into_iter())?;
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
        json: false,
    };
    while let Some(option) = args.next() {
        if option == "--json" {
            options.json = true;
            continue;
        }
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
    if !matches!(
        options.command.as_str(),
        "list" | "submit" | "status" | "watch" | "cancel" | "approve" | "reject" | "stop"
    ) {
        return Err("unknown task command; use fishnet task --help".into());
    }
    Ok(options)
}

async fn command(options: Options) -> CliResult<u8> {
    if !options.connection.exists() {
        return Err("No service connection file. Run fishnet in the project directory to start it, or use --connection PATH.".into());
    }
    let client = Client::connect(&options.connection)?;
    let id = if let Some(id) = &options.id {
        resolve(&client, id).await?
    } else {
        String::new()
    };
    let view = match options.command.as_str() {
        "list" => {
            show_tasks(client.list().await?, options.json)?;
            return Ok(0);
        }
        "stop" => {
            client.shutdown().await?;
            if options.json {
                print(&serde_json::json!({"shutdown_requested":true}))?;
            } else {
                println!(
                    "Service shutdown requested. Active tasks are cancelled; pending approvals are retained."
                );
            }
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
            client.submit(Submission { request_id:request_id.clone(), task }).await.map_err(|error|format!("Request {request_id}: {error}. Inspect the task list before retrying; no command was retried."))?
        }
        "status" => client.snapshot(&id).await?,
        "cancel" => client.cancel(&id).await?,
        "approve" | "reject" => {
            client
                .approve(
                    &id,
                    Approval {
                        expected_revision: options.revision,
                        approve: options.command == "approve",
                    },
                )
                .await?
        }
        "watch" => {
            let mut options = options;
            options.id = Some(id);
            return watch(&options).await;
        }
        _ => return Err("unknown task command".into()),
    };
    show(&view, options.json)?;
    Ok(0)
}

fn show_tasks(tasks: Vec<crate::contract::TaskSummary>, json: bool) -> CliResult<()> {
    if json {
        return print(&tasks);
    }
    let mut output = io::stdout().lock();
    if tasks.is_empty() {
        writeln!(output, "No tasks. Run fishnet to create one.")?;
    }
    for task in tasks {
        writeln!(
            output,
            "{}  {}\n  {}",
            short(&task.task_id),
            crate::display::status(task.phase, task.state, false),
            crate::display::safe(&task.objective, 160)
        )?;
    }
    Ok(())
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
                    if options.json {
                        print(&page)?;
                    } else {
                        println!(
                            "Event history gap. Loading retained events; full journal remains on disk."
                        );
                    }
                    after = page.retained_from.saturating_sub(1);
                    continue;
                }
                for event in &page.events {
                    if options.json {
                        print(event)?;
                    } else {
                        println!("{}", crate::display::describe(&event.data));
                    }
                }
                after = page.cursor;
                if page.snapshot.phase.terminal() && after >= page.snapshot.cursor {
                    show(&page.snapshot, options.json)?;
                    return Ok(exit_code(&page.snapshot));
                }
            }
            Err(error) => {
                if options.json {
                    print(&error)?;
                } else {
                    eprintln!("{error}");
                }
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
