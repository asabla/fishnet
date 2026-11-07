#![cfg(unix)]

use std::collections::VecDeque;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use fishnet_core::process::{self, CommandSpec};
use fishnet_core::protocol::{
    ErrorKind, EventData, ModelReply, OperationRequest, TaskContract, ToolCall, ToolResult,
};
use fishnet_core::provider::{ModelProvider, ProviderContext, ProviderTurn, fingerprint};
use fishnet_core::workspace::{self, Workspace, WorkspaceSpec};
use fishnet_core::{Cancellation, Limits, RunOptions, TaskState, inspect, run};
use serde_json::json;

type TestResult = Result<(), Box<dyn Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Root(PathBuf);

impl Root {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = Self(std::env::temp_dir().canonicalize()?.join(format!(
            "fishnet-m2-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&root.0)?;
        fs::write(root.0.join("work.txt"), "original\n")?;
        fs::write(root.0.join("check.txt"), "protected\n")?;
        Ok(root)
    }

    fn spec(&self) -> WorkspaceSpec {
        WorkspaceSpec {
            root: self.0.clone(),
            allowed_writes: vec!["work.txt".into()],
            required_changes: vec!["work.txt".into()],
            protected_paths: vec!["check.txt".into()],
            commands: vec![command(
                "verify",
                "/usr/bin/grep",
                &["-qx", "updated", "work.txt"],
            )],
            checks: vec!["verify".into()],
        }
    }

    fn options(&self) -> RunOptions {
        RunOptions {
            store: self.0.join("runs"),
            limits: Limits {
                deadline_ms: 5_000,
                max_provider_calls: 16,
                max_output_bytes: 8_192,
            },
            contract: TaskContract {
                objective: "Replace original with updated and pass the protected check".into(),
                expected_key: "workspace".into(),
                expected_value: None,
                policy_version: workspace::POLICY.into(),
                fixture_revision: "workspace-v1".into(),
                workspace: Some(self.spec()),
            },
            cancellation: Cancellation::default(),
        }
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Steps(VecDeque<ModelReply>);

impl ModelProvider for Steps {
    fn version(&self) -> &'static str {
        "coding-test-v1"
    }
    async fn respond(
        &mut self,
        _context: ProviderContext<'_>,
        _previous: Option<&ToolResult>,
        _cancel: &Cancellation,
        _deadline: Instant,
    ) -> Result<ProviderTurn, fishnet_core::protocol::OperationError> {
        Ok(ProviderTurn {
            reply: self
                .0
                .pop_front()
                .ok_or_else(|| fishnet_core::protocol::OperationError {
                    kind: ErrorKind::ProviderFailure,
                    message: "test script exhausted".into(),
                })?,
            observation: None,
        })
    }
}

fn command(id: &str, program: &str, args: &[&str]) -> CommandSpec {
    CommandSpec {
        id: id.into(),
        program: program.into(),
        arguments: args.iter().map(|value| (*value).into()).collect(),
        timeout_ms: 1_000,
    }
}

fn edit() -> ModelReply {
    ModelReply::ToolCall(ToolCall {
        name: "replace_text".into(),
        arguments: json!({"path":"work.txt",
        "expected_sha256":fingerprint(b"original\n"),"old":"original","new":"updated"}),
    })
}

fn final_reply() -> ModelReply {
    ModelReply::Final {
        text: "The change is ready for independent checks.".into(),
    }
}

#[test]
fn owned_coding_cycle_changes_a_real_file_and_runs_independent_checks() -> TestResult {
    let root = Root::new()?;
    let options = root.options();
    let provider = Steps(VecDeque::from([edit(), final_reply()]));
    let result = run(options, provider, |_| ())?;
    assert_eq!(result.outcome.state, TaskState::VerifiedSuccess);
    assert_eq!(fs::read_to_string(root.0.join("work.txt"))?, "updated\n");
    let record = inspect(&root.0.join("runs"), &result.run_id)?;
    assert_eq!(record.statistics.logical_tool_calls, 1);
    assert_eq!(record.statistics.process_attempts, 1);
    assert!(record.statistics.harness_us.is_some());
    assert!(matches!(&record.events.first().ok_or("no events")?.data,
        EventData::RunCreated { workspace_start:Some(start),.. } if start.starting_files.contains_key("work.txt")));
    Ok(())
}

#[test]
fn model_claim_and_zero_exit_cannot_replace_required_source_evidence() -> TestResult {
    let root = Root::new()?;
    let mut options = root.options();
    options
        .contract
        .workspace
        .as_mut()
        .ok_or("no workspace")?
        .commands = vec![command("verify", "/usr/bin/true", &[])];
    let result = run(options, Steps(VecDeque::from([final_reply()])), |_| ())?;
    assert_eq!(result.outcome.state, TaskState::Failed);
    Ok(())
}

#[test]
fn absent_independent_checks_remain_unverified() -> TestResult {
    let root = Root::new()?;
    let mut options = root.options();
    options
        .contract
        .workspace
        .as_mut()
        .ok_or("no workspace")?
        .checks
        .clear();
    let result = run(
        options,
        Steps(VecDeque::from([edit(), final_reply()])),
        |_| (),
    )?;
    assert_eq!(result.outcome.state, TaskState::UnverifiedCompletion);
    Ok(())
}

#[tokio::test]
async fn workspace_rejects_escape_symlinks_credentials_and_stale_edits() -> TestResult {
    let root = Root::new()?;
    std::os::unix::fs::symlink("/etc", root.0.join("escape"))?;
    let workspace = Workspace::open(root.spec())?;
    let control = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    for path in [
        "../outside",
        "/etc/passwd",
        "escape/passwd",
        ".git/config",
        ".fishnet/auth/account-default.json",
        ".env",
        "target/file",
    ] {
        let call = ToolCall {
            name: "read_file".into(),
            arguments: json!({"path":path,"offset":0,"max_bytes":100}),
        };
        assert!(
            matches!(workspace.execute(&call,&control,deadline,8_192).await,Err(error) if error.kind == ErrorKind::PermissionDenied)
        );
    }
    let ModelReply::ToolCall(edit) = edit() else {
        return Err("no edit".into());
    };
    fs::write(root.0.join("work.txt"), "concurrent user work\n")?;
    assert!(
        matches!(workspace.execute(&edit,&control,deadline,8_192).await,Err(error) if error.kind == ErrorKind::Conflict)
    );
    assert_eq!(
        fs::read_to_string(root.0.join("work.txt"))?,
        "concurrent user work\n"
    );
    Ok(())
}

#[tokio::test]
async fn file_pages_have_complete_hashes_and_atomic_edits_preserve_permissions() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let root = Root::new()?;
    fs::set_permissions(root.0.join("work.txt"), fs::Permissions::from_mode(0o640))?;
    let workspace = Workspace::open(root.spec())?;
    let cancellation = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let call = ToolCall {
        name: "read_file".into(),
        arguments: json!({"path":"work.txt","offset":0,"max_bytes":3}),
    };
    let ToolResult::Output { data } = workspace
        .execute(&call, &cancellation, deadline, 8_192)
        .await?
    else {
        return Err("no output".into());
    };
    assert_eq!(data.get("text"), Some(&json!("ori")));
    assert_eq!(data.get("sha256"), Some(&json!(fingerprint(b"original\n"))));
    let ModelReply::ToolCall(edit) = edit() else {
        return Err("no edit".into());
    };
    workspace
        .execute(&edit, &cancellation, deadline, 8_192)
        .await?;
    assert_eq!(
        fs::metadata(root.0.join("work.txt"))?.permissions().mode() & 0o777,
        0o640
    );
    assert!(!fs::read_dir(&root.0)?.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with(".fishnet-edit-")
    }));
    Ok(())
}

#[test]
fn check_sources_and_executables_cannot_receive_write_grants() -> TestResult {
    let root = Root::new()?;
    let mut spec = root.spec();
    spec.allowed_writes.push("check.txt".into());
    assert!(Workspace::open(spec).is_err());
    let mut spec = root.spec();
    spec.commands = vec![command(
        "verify",
        root.0.join("work.txt").to_str().ok_or("invalid path")?,
        &[],
    )];
    assert!(Workspace::open(spec).is_err());
    Ok(())
}

#[test]
fn changed_relevant_content_breaks_invalid_request_equivalence() -> TestResult {
    let root = Root::new()?;
    let workspace = Workspace::open(root.spec())?;
    let call = ToolCall {
        name: "replace_text".into(),
        arguments: json!({"path":"work.txt"}),
    };
    let before = workspace.relevant_state(&call)?;
    fs::write(root.0.join("work.txt"), "changed externally\n")?;
    assert_ne!(workspace.relevant_state(&call)?, before);
    fs::write(root.0.join("check.txt"), "weakened check\n")?;
    assert!(workspace.verify_changes().is_err());
    Ok(())
}

#[test]
fn cancellation_before_edit_is_effect_free_and_after_edit_preserves_evidence() -> TestResult {
    for after in [false, true] {
        let root = Root::new()?;
        let options = root.options();
        let cancellation = options.cancellation.clone();
        let result = run(
            options,
            Steps(VecDeque::from([edit(), final_reply()])),
            |event| {
                if (!after
                    && matches!(
                        &event.data,
                        EventData::OperationStarted {
                            request: OperationRequest::Tool { .. }
                        }
                    ))
                    || (after
                        && matches!(
                            &event.data,
                            EventData::OperationFinished {
                                result: fishnet_core::protocol::OperationResult::Tool { .. }
                            }
                        ))
                {
                    cancellation.cancel();
                }
            },
        )?;
        assert_eq!(result.outcome.state, TaskState::Cancelled);
        assert_eq!(
            fs::read_to_string(root.0.join("work.txt"))?,
            if after { "updated\n" } else { "original\n" }
        );
        assert!(
            inspect(&root.0.join("runs"), &result.run_id)?
                .pending_operation_ids
                .is_empty()
        );
    }
    Ok(())
}

#[tokio::test]
async fn process_outputs_are_bounded_and_negative_results_are_not_transport_errors() -> TestResult {
    let root = Root::new()?;
    let cancellation = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let output = process::execute(
        &command("large", "/usr/bin/printf", &["%5000s", "x"]),
        &root.0,
        &cancellation,
        deadline,
        64,
    )
    .await?;
    assert_eq!(output.exit_code, Some(0));
    assert!(output.truncated);
    assert!(output.stdout.len() <= 64);
    let output = process::execute(
        &command("negative", "/usr/bin/grep", &["-q", "absent", "work.txt"]),
        &root.0,
        &cancellation,
        deadline,
        64,
    )
    .await?;
    assert_eq!(output.exit_code, Some(1));
    Ok(())
}

#[tokio::test]
async fn cancellation_kills_the_owned_process_group_before_returning() -> TestResult {
    let root = Root::new()?;
    let cancellation = Cancellation::default();
    let cancel = cancellation.clone();
    let directory = root.0.clone();
    let trigger = tokio::spawn(async move {
        for _ in 0..100 {
            if directory.join("started").exists() {
                cancel.cancel();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancel.cancel();
    });
    let spec = command(
        "tree",
        "/bin/sh",
        &[
            "-c",
            "printf ready > started; (sleep 1; printf escaped > late) & wait",
        ],
    );
    let result = process::execute(
        &spec,
        &root.0,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
        64,
    )
    .await;
    trigger.await?;
    assert!(matches!(result,Err(error) if error.kind == ErrorKind::Cancelled));
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    assert!(!root.0.join("late").exists());
    Ok(())
}

#[tokio::test]
async fn process_deadline_and_environment_are_enforced() -> TestResult {
    let root = Root::new()?;
    let cancellation = Cancellation::default();
    let spec = command("deadline", "/bin/sleep", &["2"]);
    let result = process::execute(
        &spec,
        &root.0,
        &cancellation,
        Instant::now() + Duration::from_millis(30),
        64,
    )
    .await;
    assert!(matches!(result,Err(error) if error.kind == ErrorKind::DeadlineExceeded));
    let output = process::execute(
        &command("environment", "/usr/bin/env", &[]),
        &root.0,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
        8_192,
    )
    .await?;
    for line in output.stdout.lines() {
        let key = line.split('=').next().unwrap_or_default();
        assert!(
            [
                "PATH",
                "HOME",
                "CARGO_HOME",
                "RUSTUP_HOME",
                "RUSTUP_TOOLCHAIN",
                "TMPDIR",
                "CARGO_NET_OFFLINE",
                "GIT_TERMINAL_PROMPT"
            ]
            .contains(&key)
        );
    }
    Ok(())
}

#[tokio::test]
async fn file_pages_use_available_budget_and_bound_json_escaping() -> TestResult {
    let root = Root::new()?;
    let workspace = Workspace::open(root.spec())?;
    let control = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    for text in ["a".repeat(12_000), "\u{0001}".repeat(12_000)] {
        fs::write(root.0.join("work.txt"), &text)?;
        let call = ToolCall {
            name: "read_file".into(),
            arguments: json!({"path":"work.txt","offset":0,"max_bytes":20_000}),
        };
        let result = workspace.execute(&call, &control, deadline, 8_192).await?;
        assert!(serde_json::to_vec(&result)?.len() <= 8_192);
        let ToolResult::Output { data } = result else {
            return Err("no page".into());
        };
        let page = data
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or("no text")?;
        assert!(page.len() > 682);
        assert!(text.starts_with(page));
        assert_eq!(data.get("next_offset"), Some(&json!(page.len())));
        assert_eq!(
            data.get("sha256"),
            Some(&json!(fingerprint(text.as_bytes())))
        );
    }
    Ok(())
}
