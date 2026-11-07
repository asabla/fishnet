//! Local startup for the interactive client. The child remains the task owner.
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{
    client::Client,
    contract::{ApiError, Result},
};

/// Connect to this working tree's service, or start the same executable in service mode.
///
/// Coding preparation is enabled, but every coding task still requires grant approval.
/// No account or provider request is made at startup. Explicit connection files bypass this.
///
/// # Errors
/// Returns unsafe storage, process startup, listener, or service readiness errors.
pub fn ensure(executable: &Path) -> Result<bool> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ApiError::invalid("cannot initialize the local client"))?;
    runtime.block_on(ensure_async(executable))
}

async fn ready() -> bool {
    let Ok(client) = Client::connect(Path::new(".fishnet/service/connection.json")) else {
        return false;
    };
    tokio::time::timeout(Duration::from_secs(1), client.list())
        .await
        .is_ok_and(|r| r.is_ok())
}

async fn ensure_async(executable: &Path) -> Result<bool> {
    if ready().await {
        return Ok(false);
    }
    let log = super::store::launch_log(Path::new(".fishnet/service"))?;
    let errors = log.try_clone().map_err(|_| ApiError::storage())?;
    let mut command = Command::new(executable);
    command
        .args(["serve", "--allow-code"])
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(errors);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Terminal Ctrl-C must detach the client without reaching the service.
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| ApiError::invalid("cannot start the local service"))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if ready().await {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(true);
        }
        if child.try_wait().map_err(|_| ApiError::storage())?.is_some() {
            // Another starting client may have won the exclusive store lock.
            if ready().await {
                return Ok(false);
            }
            return Err(startup_error());
        }
        if Instant::now() >= deadline {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Err(startup_error());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
fn startup_error() -> ApiError {
    ApiError::new(
        "service_start",
        "local service did not become ready; see .fishnet/service/daemon.log or run fishnet serve --allow-code",
    )
}
