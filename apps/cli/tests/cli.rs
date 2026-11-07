use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use fishnet_core::protocol::{Event, EventData, OperationRequest};
use fishnet_core::storage::record_path;
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn Error>>;
static NEXT_STORE: AtomicU64 = AtomicU64::new(0);

struct Store(PathBuf);

impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "fishnet-cli-test-{}-{}",
            std::process::id(),
            NEXT_STORE.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn command(operation: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fishnet"));
        command.arg(operation);
        command
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningChild(Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn read_until_provider(
    reader: &mut BufReader<ChildStdout>,
    turn: u32,
) -> Result<String, Box<dyn Error>> {
    for line in reader.lines() {
        let event: Event = serde_json::from_str(&line?)?;
        if event.turn == turn
            && matches!(
                event.data,
                EventData::OperationStarted {
                    request: OperationRequest::Provider { .. }
                }
            )
        {
            return Ok(event.task_id);
        }
    }
    Err("process ended before the expected provider attempt".into())
}

#[test]
fn cli_outcomes_and_restart_inspection_are_machine_readable() -> TestResult {
    let store = Store::new();
    for (scenario, code, state) in [
        ("success", 0, "verified_success"),
        ("unverified", 3, "unverified_completion"),
        ("check-failure", 2, "failed"),
        ("provider-failure", 2, "failed"),
        ("repeated-invalid", 3, "unverified_completion"),
        ("negative", 0, "verified_success"),
    ] {
        let output = Store::command("run")
            .args(["--scenario", scenario, "--store"])
            .arg(&store.0)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let lines = String::from_utf8(output.stdout)?;
        let last: Value = serde_json::from_str(lines.lines().last().ok_or("no events")?)?;
        assert_eq!(last.pointer("/type"), Some(&json!("task_finished")));
        assert_eq!(last.pointer("/outcome/state"), Some(&json!(state)));
        let run_id = last
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or("no task ID")?;
        let path = record_path(&store.0, run_id)?;
        let before = fs::read(&path)?;
        let inspection = Store::command("inspect")
            .args([run_id, "--store"])
            .arg(&store.0)
            .output()?;
        assert!(inspection.status.success());
        let record: Value = serde_json::from_slice(&inspection.stdout)?;
        assert_eq!(record.pointer("/state"), Some(&json!(state)));
        assert_eq!(record.pointer("/incomplete"), Some(&json!(false)));
        assert_eq!(fs::read(&path)?, before);
    }
    Ok(())
}

#[test]
fn killed_process_leaves_inspectable_evidence_and_does_not_replay() -> TestResult {
    let store = Store::new();
    let mut child = RunningChild(
        Store::command("run")
            .args([
                "--step-delay-ms",
                "500",
                "--deadline-ms",
                "10000",
                "--store",
            ])
            .arg(&store.0)
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut reader = BufReader::new(child.0.stdout.take().ok_or("no child stdout")?);
    let run_id = read_until_provider(&mut reader, 2)?;
    child.0.kill()?;
    child.0.wait()?;
    let path = record_path(&store.0, &run_id)?;
    let before = fs::read(&path)?;
    let output = Store::command("inspect")
        .args([&run_id, "--store"])
        .arg(&store.0)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(record.pointer("/state"), Some(&json!("running")));
    assert_eq!(record.pointer("/incomplete"), Some(&json!(true)));
    assert_eq!(record.pointer("/outcome"), Some(&Value::Null));
    assert_eq!(
        record.pointer("/statistics/logical_tool_calls"),
        Some(&json!(1))
    );
    assert_eq!(record.pointer("/statistics/tool_results"), Some(&json!(1)));
    assert_eq!(
        record
            .get("pending_operation_ids")
            .and_then(Value::as_array)
            .ok_or("no pending operations")?
            .len(),
        1
    );
    assert_eq!(fs::read(path)?, before);
    Ok(())
}

#[test]
#[cfg(unix)]
fn sigint_persists_cancellation_and_exits_130() -> TestResult {
    let store = Store::new();
    let mut child = RunningChild(
        Store::command("run")
            .args([
                "--step-delay-ms",
                "10000",
                "--deadline-ms",
                "20000",
                "--store",
            ])
            .arg(&store.0)
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut reader = BufReader::new(child.0.stdout.take().ok_or("no child stdout")?);
    let run_id = read_until_provider(&mut reader, 1)?;
    // The OS signal utility is test infrastructure, not a harness execution tool.
    assert!(
        Command::new("/bin/kill")
            .args(["-INT", &child.0.id().to_string()])
            .status()?
            .success()
    );
    let mut terminal = None;
    for line in reader.lines() {
        let event: Event = serde_json::from_str(&line?)?;
        if let EventData::TaskFinished { outcome } = event.data {
            terminal = Some(outcome);
        }
    }
    assert_eq!(child.0.wait()?.code(), Some(130));
    assert_eq!(
        terminal.ok_or("missing terminal event")?.state,
        fishnet_core::TaskState::Cancelled
    );
    let record = fishnet_core::inspect(&store.0, &run_id)?;
    assert_eq!(record.state, fishnet_core::TaskState::Cancelled);
    assert!(record.pending_operation_ids.is_empty());
    assert_eq!(record.statistics.logical_tool_calls, 0);
    Ok(())
}

#[test]
fn cli_rejects_invalid_options_without_creating_records() -> TestResult {
    let store = Store::new();
    for arguments in [
        vec!["run", "--scenario", "live-provider"],
        vec!["run", "--max-output-bytes", "9999999"],
        vec!["run", "--max-provider-calls", "0"],
        vec!["run", "--unknown", "value"],
        vec!["inspect", "../escape"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fishnet"))
            .args(arguments)
            .arg("--store")
            .arg(&store.0)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    assert!(!store.0.exists());
    Ok(())
}
