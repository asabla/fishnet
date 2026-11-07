use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::{ErrorKind, OperationError};

/// One shared cancellation flag and wakeup. The runtime remains the sole state owner.
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    state: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    requested: Mutex<bool>,
    wake: Condvar,
}

impl Cancellation {
    pub fn cancel(&self) {
        *self
            .state
            .requested
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.state.wake.notify_all();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        *self
            .state
            .requested
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn check(&self, deadline: Instant) -> Result<(), OperationError> {
        if self.is_cancelled() {
            return Err(OperationError {
                kind: ErrorKind::Cancelled,
                message: "cancellation requested".into(),
            });
        }
        if Instant::now() >= deadline {
            return Err(OperationError {
                kind: ErrorKind::DeadlineExceeded,
                message: "task deadline exceeded".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn wait(&self, delay: Duration, deadline: Instant) -> Result<(), OperationError> {
        let end = Instant::now() + delay;
        let state = &self.state;
        loop {
            self.check(deadline)?;
            let now = Instant::now();
            if now >= end {
                return Ok(());
            }
            let cancelled = state
                .requested
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *cancelled {
                continue;
            }
            let _guard = state
                .wake
                .wait_timeout(cancelled, end.min(deadline).saturating_duration_since(now))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}
