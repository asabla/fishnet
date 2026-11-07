use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

use crate::control::Cancellation;
use crate::protocol::{BoundedText, ErrorKind, OperationError};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    pub id: String,
    pub program: std::path::PathBuf,
    pub arguments: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub duration_us: u64,
}

struct OwnedChild {
    child: Child,
    group: Option<i32>,
}

impl OwnedChild {
    fn terminate(&mut self) {
        #[cfg(unix)]
        if let Some(group) = self.group.take() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(group),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        let _ = self.child.start_kill();
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Execute one host-selected argv without a shell. No provider credential is inherited.
///
/// # Errors
/// Returns policy, spawn, pipe, cancellation, or deadline errors. Interrupted work is not retried.
pub async fn execute(
    spec: &CommandSpec,
    root: &Path,
    cancellation: &Cancellation,
    deadline: Instant,
    capture_limit: usize,
) -> Result<ProcessOutput, OperationError> {
    validate(spec)?;
    cancellation.check(deadline)?;
    let started = Instant::now();
    let deadline = deadline.min(started + std::time::Duration::from_millis(spec.timeout_ms));
    let mut command = build_command(spec, root);
    let child = command
        .spawn()
        .map_err(|_| error("could not start the authorized program"))?;
    let group = child.id().and_then(|id| i32::try_from(id).ok());
    let mut owned = OwnedChild { child, group };
    let result = cancellation
        .interrupt(deadline, Box::pin(collect(&mut owned.child, capture_limit)))
        .await;
    owned.terminate();
    let _ = owned.child.wait().await;
    let mut output = result?.map_err(|mut failure| {
        failure.kind = ErrorKind::UnknownEffect;
        failure.message =
            "process started but its result is unavailable; inspect effects before retrying".into();
        failure
    })?;
    output.duration_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    Ok(output)
}

/// # Errors
/// Rejects relative programs, oversized argv, and unsupported process bounds.
pub fn validate(spec: &CommandSpec) -> Result<(), OperationError> {
    if !cfg!(unix) {
        return Err(error(
            "controlled process execution is supported only on Unix in M2",
        ));
    }
    if !spec.program.is_absolute()
        || !spec.program.is_file()
        || spec.id.is_empty()
        || spec.id.len() > 64
        || spec.arguments.len() > 32
        || spec.arguments.iter().map(String::len).sum::<usize>() > 8_192
        || !(1..=300_000).contains(&spec.timeout_ms)
    {
        return Err(OperationError { kind: ErrorKind::PermissionDenied,
            message: "command requires an absolute existing program, bounded argv, and timeout 1..=300000".into() });
    }
    Ok(())
}

fn build_command(spec: &CommandSpec, root: &Path) -> Command {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.arguments)
        .current_dir(root)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Only host toolchain settings are passed. API keys and proxy credentials are excluded.
    for name in [
        "PATH",
        "HOME",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "TMPDIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("CARGO_NET_OFFLINE", "true")
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(unix)]
    command.process_group(0);
    command
}

async fn collect(child: &mut Child, limit: usize) -> Result<ProcessOutput, OperationError> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error("stdout pipe unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| error("stderr pipe unavailable"))?;
    let (stdout, stderr, status) = tokio::join!(
        read_bounded(stdout, limit),
        read_bounded(stderr, limit),
        child.wait()
    );
    let (stdout, out_truncated) = stdout?;
    let (stderr, err_truncated) = stderr?;
    Ok(ProcessOutput {
        exit_code: status
            .map_err(|_| error("could not collect process status"))?
            .code(),
        stdout,
        stderr,
        truncated: out_truncated || err_truncated,
        duration_us: 0,
    })
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<(String, bool), OperationError> {
    let mut saved = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 4_096];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|_| error("could not read process pipe"))?;
        if count == 0 {
            break;
        }
        let available = limit.saturating_sub(saved.len()).min(count);
        saved.extend(buffer.get(..available).unwrap_or_default());
        truncated |= available < count;
    }
    let text = String::from_utf8_lossy(&saved);
    let bounded = BoundedText::new(&text, limit);
    Ok((bounded.text, truncated || bounded.truncated))
}

fn error(message: &str) -> OperationError {
    OperationError {
        kind: ErrorKind::ExecutionFailure,
        message: message.into(),
    }
}
