use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use fishnet_core::{
    Cancellation, Limits, RunOptions, Scenario, ScriptedProvider, TaskState, inspect, run,
};

const HELP: &str = "fishnet M1: deterministic local harness
Usage:
  fishnet run [--scenario NAME] [--store DIR] [--deadline-ms N]
              [--step-delay-ms N] [--max-output-bytes N] [--max-provider-calls N]
  fishnet inspect RUN_ID [--store DIR]

Scenarios: success, unverified, check-failure, repeated-invalid, negative, provider-failure
Default store: .fishnet/runs. Output: JSON events for run; JSON record for inspect.
Ctrl-C requests cancellation. Inspection never executes tools.
Exit codes: 0 verified/inspection, 2 failed/error, 3 unverified, 130 cancelled.";

#[derive(Debug)]
struct Arguments {
    command: String,
    run_id: Option<String>,
    store: PathBuf,
    scenario: Scenario,
    limits: Limits,
    delay_ms: u64,
}

fn parse_arguments() -> Result<Option<Arguments>, String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Ok(None);
    };
    if command == "--help" || command == "-h" {
        return Ok(None);
    }
    if command != "run" && command != "inspect" {
        return Err("expected run or inspect".into());
    }
    let mut parsed = Arguments {
        run_id: if command == "inspect" {
            Some(args.next().ok_or("inspect requires a run ID")?)
        } else {
            None
        },
        command,
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
        if parsed.command == "inspect" {
            return Err(format!("unsupported inspection option: {flag}"));
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

fn execute(args: Arguments) -> Result<u8, Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let mut output = stdout;
    if let Some(run_id) = args.run_id {
        serde_json::to_writer(&mut output, &inspect(&args.store, &run_id)?)?;
        writeln!(output)?;
        return Ok(0);
    }

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
    let result = match parse_arguments() {
        Ok(Some(args)) => execute(args),
        Ok(None) => writeln!(io::stdout(), "{HELP}")
            .map(|()| 0)
            .map_err(Into::into),
        Err(message) => Err(message.into()),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(io::stderr(), "fishnet: {error}");
            ExitCode::from(2)
        }
    }
}
