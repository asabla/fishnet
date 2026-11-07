mod account;
mod coding;
mod inspection;

use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use fishnet_core::{Cancellation, Limits, RunOptions, Scenario, ScriptedProvider, TaskState, run};

const HELP: &str = "fishnet: independent Rust coding harness

  fishnet                         Open the terminal interface; start a local service if needed
  fishnet --task tasks/FILE.json   Open a reviewed task file
  fishnet attach TASK_ID           Reattach to a task
  fishnet tasks                   List tasks
  fishnet status TASK_ID           Show task state and approval details
  fishnet watch TASK_ID            Follow task output
  fishnet cancel TASK_ID           Cancel a task
  fishnet stop                    Stop the local service and cancel active tasks
  fishnet auth login|status|logout Connect a ChatGPT account
  fishnet models                  List models available to that account

Start fishnet in the project directory. Coding requires review and approval.
The local service stays alive when the terminal client detaches.
Use --help on a command for options. Use --json on task/account commands for scripts.

Advanced:
  fishnet run|tui [--color auto|always|never] [--connection PATH] [--attach ID]
  fishnet serve [--allow-code] [SERVICE_OPTIONS]
  fishnet task list|submit|status|watch|cancel|approve|reject [ID] [OPTIONS]
  fishnet code --task PATH [PROVIDER_OPTIONS]  Standalone coding utility
  fishnet fixture [--scenario NAME]          Offline JSON fixture utility
  fishnet inspect RUN_ID [--summary]         Offline JSON journal inspection

Exit codes: 0 successful command/verified task, 2 error, 3 unverified, 130 cancelled.
A successful submission or inspection does not establish a successful task.";

#[derive(Debug)]
struct Arguments {
    store: PathBuf,
    scenario: Scenario,
    limits: Limits,
    delay_ms: u64,
}

fn parse_fixture_arguments() -> Result<Option<Arguments>, String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Ok(None);
    };
    if command == "--help" || command == "-h" {
        return Ok(None);
    }
    if command != "fixture" {
        return Err("expected fixture".into());
    }
    let mut parsed = Arguments {
        store: ".fishnet/runs".into(),
        scenario: Scenario::Success,
        limits: Limits::default(),
        delay_ms: 0,
    };
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if flag == "--store" {
            parsed.store = value.into();
            continue;
        }
        match flag.as_str() {
            "--scenario" => {
                parsed.scenario = Scenario::parse(&value)
                    .ok_or_else(|| format!("scenario must be one of {}", Scenario::NAMES))?;
            }
            "--deadline-ms" => {
                parsed.limits.deadline_ms = value.parse().map_err(|_| "invalid deadline")?;
            }
            "--step-delay-ms" => {
                parsed.delay_ms = value.parse().map_err(|_| "invalid step delay")?;
            }
            "--max-output-bytes" => {
                parsed.limits.max_output_bytes =
                    value.parse().map_err(|_| "invalid output limit")?;
            }
            "--max-provider-calls" => {
                parsed.limits.max_provider_calls =
                    value.parse().map_err(|_| "invalid call limit")?;
            }
            _ => return Err(format!("unsupported option: {flag}")),
        }
    }
    parsed.limits.validate()?;
    Ok(Some(parsed))
}

fn execute_fixture(args: Arguments) -> Result<u8, Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let mut output = stdout;
    let cancellation = Cancellation::default();
    let signal_cancellation = cancellation.clone();
    ctrlc::set_handler(move || signal_cancellation.cancel())?;
    let (provider, contract) =
        ScriptedProvider::scenario(args.scenario, Duration::from_millis(args.delay_ms))?;
    let options = RunOptions {
        store: args.store,
        limits: args.limits,
        contract,
        cancellation,
    };
    // A closed output pipe must not prevent a durable terminal outcome.
    let mut output_error = None;
    let result = run(options, provider, |event| {
        if output_error.is_none() {
            output_error = serde_json::to_writer(&mut output, event)
                .map_err(io::Error::other)
                .and_then(|()| writeln!(output))
                .and_then(|()| output.flush())
                .err();
        }
    })?;
    if let Some(error) = output_error {
        return Err(error.into());
    }
    Ok(match result.outcome.state {
        TaskState::VerifiedSuccess => 0,
        TaskState::UnverifiedCompletion => 3,
        TaskState::Cancelled => 130,
        _ => 2,
    })
}

fn main() -> ExitCode {
    report(dispatch())
}
fn dispatch() -> Result<u8, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let first = args.next();
    match first.as_deref() {
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => {
            fishnet_tui::terminal::launch(args).map(|()| 0)
        }
        None | Some("--help" | "-h" | "help") => {
            writeln!(io::stdout(), "{HELP}")?;
            Ok(0)
        }
        Some("--version" | "-V") => {
            writeln!(io::stdout(), "fishnet {}", env!("CARGO_PKG_VERSION"))?;
            Ok(0)
        }
        Some("run" | "tui") => fishnet_tui::terminal::launch(args).map(|()| 0),
        Some(
            "--task" | "--attach" | "--model" | "--account" | "--connection" | "--no-start"
            | "--color",
        ) => fishnet_tui::terminal::launch(first.into_iter().chain(args)).map(|()| 0),
        Some("attach") => {
            let id = args
                .next()
                .ok_or("attach requires a task ID; use fishnet tasks")?;
            if matches!(id.as_str(), "--help" | "-h") {
                return fishnet_tui::terminal::launch(std::iter::once("--help".into())).map(|()| 0);
            }
            fishnet_tui::terminal::launch(["--attach".into(), id].into_iter().chain(args))
                .map(|()| 0)
        }
        Some("tasks") => fishnet_service::cli::execute(std::iter::once("list".into()).chain(args)),
        Some("status" | "watch" | "cancel" | "stop") => {
            fishnet_service::cli::execute(first.into_iter().chain(args))
        }
        Some("serve") => fishnet_service::daemon::launch(args).map(|()| 0),
        Some("task") => fishnet_service::cli::execute(args),
        Some("auth") => account::execute(args, false),
        Some("models") => account::execute(args, true),
        Some(command @ ("code" | "inspect" | "fixture")) => {
            let args = args.collect::<Vec<_>>();
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
            {
                advanced_help(command)?;
                Ok(0)
            } else {
                match command {
                    "code" => coding::execute(args.into_iter()),
                    "inspect" => inspection::execute(args.into_iter()),
                    _ => run_fixture(),
                }
            }
        }
        Some(command) => Err(format!("unknown command: {command}; use fishnet --help").into()),
    }
}

fn advanced_help(command: &str) -> io::Result<()> {
    let text = match command {
        "code" => {
            "fishnet code --task PATH [--root DIR] [--model NAME] [--account LABEL]\n             [--provider chatgpt|openai|responses] [--endpoint URL] [--key-env NAME]\n             [--auth-dir DIR] [--store DIR]\nStandalone JSON coding utility. Use fishnet for interactive tasks and approvals."
        }
        "inspect" => {
            "fishnet inspect RUN_ID [--store DIR] [--summary]\nRead a local run journal without a service, account, or tool replay. Output is JSON."
        }
        _ => {
            "fishnet fixture [--scenario NAME] [--store DIR] [--deadline-ms N]\n            [--step-delay-ms N] [--max-output-bytes N] [--max-provider-calls N]\nOffline JSON fixture utility. Scenarios: success, unverified, check-failure,\nrepeated-invalid, negative, provider-failure. Default store: .fishnet/runs."
        }
    };
    writeln!(io::stdout(), "{text}")
}

fn run_fixture() -> Result<u8, Box<dyn std::error::Error>> {
    match parse_fixture_arguments() {
        Ok(Some(args)) => execute_fixture(args),
        Ok(None) => writeln!(io::stdout(), "{HELP}")
            .map(|()| 0)
            .map_err(Into::into),
        Err(message) => Err(message.into()),
    }
}

fn report(result: Result<u8, Box<dyn std::error::Error>>) -> ExitCode {
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(io::stderr(), "fishnet: {error}");
            if error
                .downcast_ref::<fishnet_core::protocol::OperationError>()
                .is_some_and(|error| error.kind == fishnet_core::protocol::ErrorKind::Cancelled)
            {
                ExitCode::from(130)
            } else {
                ExitCode::from(2)
            }
        }
    }
}
