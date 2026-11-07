mod conversation;
mod draft;
mod execution;
pub mod local;
mod store;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
pub use store::{Connection, read_connection};

use fishnet_core::{Cancellation, Event, inspect};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, watch};

use self::store::{Receipt, Store};
use crate::contract::validate_key;
use crate::contract::{
    ApiError, Approval, EventPage, PAGE_EVENTS, Phase, RETAINED_EVENTS, Result, RunView,
    Submission, TaskRequest, TaskSummary, TaskView,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProviderAccess {
    Fixture,
    Chatgpt {
        auth_directory: PathBuf,
    },
    ApiKey {
        endpoint: String,
        key_env: String,
        default_model: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub directory: PathBuf,
    pub runs: PathBuf,
    pub allow_code: bool,
    pub provider: ProviderAccess,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            root: ".".into(),
            directory: ".fishnet/service".into(),
            runs: ".fishnet/runs".into(),
            allow_code: false,
            provider: ProviderAccess::Chatgpt {
                auth_directory: ".fishnet/auth".into(),
            },
        }
    }
}

struct Managed {
    pending_since: Option<std::time::Instant>,
    receipt: Receipt,
    cancellation: Cancellation,
    changed: watch::Sender<u64>,
}
struct State {
    tasks: BTreeMap<String, Managed>,
    poisoned: bool,
    closing: bool,
}
struct Inner {
    config: Config,
    store: Arc<Store>,
    state: Mutex<State>,
    slots: Arc<Semaphore>,
    commands: Arc<Semaphore>,
    stop: Cancellation,
}
#[derive(Clone)]
pub struct Service {
    inner: Arc<Inner>,
}
impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service").finish_non_exhaustive()
    }
}

impl Service {
    /// Open one exclusive service owner and reconcile receipts without replaying work.
    ///
    /// # Errors
    /// Rejects invalid roots, unsafe storage, duplicate owners, and corrupt receipts.
    pub async fn open(mut config: Config) -> Result<Self> {
        let (config, store, receipts) = tokio::task::spawn_blocking(move || {
            config.root = config
                .root
                .canonicalize()
                .map_err(|_| ApiError::invalid("workspace root unavailable"))?;
            config.directory = rooted(&config.root, config.directory);
            config.runs = rooted(&config.root, config.runs);
            if let ProviderAccess::Chatgpt { auth_directory } = &mut config.provider {
                *auth_directory = rooted(&config.root, auth_directory.clone());
            }
            let store = Store::open(config.directory.clone())?;
            let mut receipts = store.load()?;
            for receipt in &mut receipts {
                if matches!(receipt.phase, Phase::Accepted | Phase::Executing) {
                    receipt.phase =
                        if inspect(&config.runs, &receipt.task_id).is_ok_and(|r| !r.incomplete) {
                            Phase::Finished
                        } else {
                            Phase::Interrupted
                        };
                    receipt.message = Some("service restarted; no operation was replayed".into());
                    store.save(receipt)?;
                }
            }
            Ok((config, store, receipts))
        })
        .await
        .map_err(|_| ApiError::storage())??;
        let tasks = receipts
            .into_iter()
            .map(|receipt| {
                (
                    receipt.task_id.clone(),
                    Managed {
                        receipt,
                        pending_since: None,
                        cancellation: Cancellation::default(),
                        changed: watch::channel(0).0,
                    },
                )
            })
            .collect();
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                store: Arc::new(store),
                state: Mutex::new(State {
                    tasks,
                    poisoned: false,
                    closing: false,
                }),
                slots: Arc::new(Semaphore::new(4)),
                commands: Arc::new(Semaphore::new(16)),
                stop: Cancellation::default(),
            }),
        })
    }

    pub(crate) fn token(&self) -> &str {
        self.inner.store.token()
    }

    /// Publish a private connection file after the listener has bound.
    ///
    /// # Errors
    /// Returns a durable storage error.
    pub async fn publish(&self, endpoint: String) -> Result<PathBuf> {
        let store = self.inner.store.clone();
        tokio::task::spawn_blocking(move || store.publish(endpoint))
            .await
            .map_err(|_| ApiError::storage())?
    }

    /// Accept a durable, idempotent submission. Client disconnect does not cancel it.
    ///
    /// # Errors
    /// Returns invalid input, capacity, conflicting identity, or persistence errors.
    pub async fn submit(&self, submission: Submission) -> Result<TaskView> {
        let service = self.clone();
        self.own_command(async move { service.submit_owned(submission).await })
            .await
    }

    async fn submit_owned(&self, submission: Submission) -> Result<TaskView> {
        validate_key(&submission.request_id)?;
        let bytes =
            serde_json::to_vec(&submission).map_err(|_| ApiError::invalid("invalid submission"))?;
        if bytes.len() > 16_384 {
            return Err(ApiError::invalid("submission exceeds 16384 bytes"));
        }
        let fingerprint = fishnet_core::provider::fingerprint(&bytes);
        let mut state = self.inner.state.lock().await;
        if state.poisoned {
            return Err(ApiError::storage());
        }
        if state.closing {
            return Err(ApiError::new("capacity", "service is shutting down"));
        }
        if let Some(existing) = state
            .tasks
            .values()
            .find(|entry| entry.receipt.submission.request_id == submission.request_id)
        {
            if existing.receipt.fingerprint != fingerprint {
                return Err(ApiError::new(
                    "conflict",
                    "request ID already identifies another submission",
                ));
            }
            let receipt = existing.receipt.clone();
            drop(state);
            return self.view(receipt).await;
        }
        if state.tasks.len() >= 128 {
            return Err(ApiError::new(
                "capacity",
                "service stores at most 128 task receipts; use a new service store after archival",
            ));
        }
        let prepared = self.prepare(&submission.task).await?;
        let task_id = crate::new_id()?;
        let (conversation, inherited) = self
            .prepare_conversation(&state, &submission.task, &task_id)
            .await?;
        let phase = if prepared.is_some() || (conversation.is_some() && !inherited) {
            Phase::AwaitingApproval
        } else {
            Phase::Accepted
        };
        let receipt = Receipt {
            conversation,
            schema_version: 1,
            decision: inherited.then_some(true),
            approval_wait_ms: if phase == Phase::AwaitingApproval {
                None
            } else {
                Some(0)
            },
            task_id,
            submission,
            fingerprint,
            phase,
            message: None,
            prepared,
            provider: self.inner.config.provider.clone(),
        };
        self.commit_and_schedule(
            &mut state,
            receipt.clone(),
            if phase == Phase::Accepted {
                Some(self.slot()?)
            } else {
                None
            },
        )
        .await?;
        drop(state);
        self.view(receipt).await
    }

    async fn prepare(
        &self,
        request: &TaskRequest,
    ) -> Result<Option<fishnet_core::workspace::TaskFile>> {
        if let Some((model, account)) = request.model_account() {
            if !self.inner.config.allow_code {
                return Err(ApiError::new(
                    "forbidden",
                    "service provider tasks are disabled; stop the service, then run fishnet again or start fishnet serve --allow-code",
                ));
            }
            validate_key(account)?;
            if model
                .as_ref()
                .is_some_and(|m| m.is_empty() || m.len() > 128)
            {
                return Err(ApiError::invalid("invalid model name"));
            }
        }
        if matches!(request, TaskRequest::Code { .. } | TaskRequest::Rust { .. })
            && self.inner.config.provider == ProviderAccess::Fixture
        {
            return Err(ApiError::invalid(
                "fixture provider supports conversations only",
            ));
        }
        match request {
            TaskRequest::Chat { .. } => Ok(None),
            TaskRequest::Fixture {
                scenario,
                delay_ms,
                limits,
            } => {
                limits.validate().map_err(ApiError::invalid)?;
                if *delay_ms > 300_000 || fishnet_core::Scenario::parse(scenario).is_none() {
                    return Err(ApiError::invalid("invalid fixture scenario or delay"));
                }
                Ok(None)
            }
            TaskRequest::Rust {
                objective, files, ..
            } => {
                let root = self.inner.config.root.clone();
                let objective = objective.clone();
                let files = files.clone();
                tokio::task::spawn_blocking(move || {
                    draft::prepare(root, objective, files).map(Some)
                })
                .await
                .map_err(|_| ApiError::storage())?
            }
            TaskRequest::Code { task_path, .. } => {
                let root = self.inner.config.root.clone();
                let path = PathBuf::from(task_path);
                if path.is_absolute()
                    || !path.starts_with("tasks")
                    || !path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    return Err(ApiError::invalid(
                        "service tasks must be reviewed relative files under tasks/",
                    ));
                }
                tokio::task::spawn_blocking(move || {
                    let path = root
                        .join(path)
                        .canonicalize()
                        .map_err(|_| ApiError::invalid("task file unavailable"))?;
                    if !path.starts_with(root.join("tasks")) {
                        return Err(ApiError::invalid("task file escapes tasks/"));
                    }
                    let task = fishnet_core::workspace::load_task(&path, Some(&root))
                        .map_err(|e| ApiError::invalid(&e.message))?;
                    fishnet_core::workspace::Workspace::open(task.workspace.clone())
                        .map_err(|e| ApiError::invalid(&e.message))?;
                    Ok(Some(task))
                })
                .await
                .map_err(|_| ApiError::storage())?
            }
        }
    }

    fn slot(&self) -> Result<OwnedSemaphorePermit> {
        self.inner
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError::new("capacity", "four tasks are already active"))
    }

    async fn persist(&self, receipt: Receipt) -> Result<()> {
        let store = self.inner.store.clone();
        tokio::task::spawn_blocking(move || store.save(&receipt))
            .await
            .map_err(|_| ApiError::storage())?
    }

    /// Resolve one pending task grant exactly once, across all clients.
    ///
    /// # Errors
    /// Returns missing task, stale approval, execution capacity, or persistence errors.
    pub async fn approve(&self, id: &str, approval: Approval) -> Result<TaskView> {
        let service = self.clone();
        let id = id.to_owned();
        self.own_command(async move { service.approve_owned(&id, approval).await })
            .await
    }

    async fn approve_owned(&self, id: &str, approval: Approval) -> Result<TaskView> {
        let mut state = self.inner.state.lock().await;
        if state.poisoned {
            return Err(ApiError::storage());
        }
        if state.closing {
            return Err(ApiError::new("capacity", "service is shutting down"));
        }
        let entry = state.tasks.get(id).ok_or_else(not_found)?;
        if entry.receipt.phase != Phase::AwaitingApproval || approval.expected_revision != 1 {
            return Err(ApiError::new(
                "conflict",
                "approval is no longer pending at this revision",
            ));
        }
        if approval.approve
            && entry.receipt.prepared.is_some()
            && state.tasks.values().any(|other| {
                other.receipt.prepared.is_some()
                    && matches!(other.receipt.phase, Phase::Accepted | Phase::Executing)
            })
        {
            return Err(ApiError::new(
                "capacity",
                "another coding task is active for this workspace; finish or cancel it before approving another",
            ));
        }
        let mut receipt = entry.receipt.clone();
        receipt.decision = Some(approval.approve);
        receipt.approval_wait_ms = entry
            .pending_since
            .map(|started| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        receipt.phase = if approval.approve {
            Phase::Accepted
        } else {
            Phase::Rejected
        };
        self.commit_and_schedule(
            &mut state,
            receipt.clone(),
            if approval.approve {
                Some(self.slot()?)
            } else {
                None
            },
        )
        .await?;
        drop(state);
        self.view(receipt).await
    }

    /// Cancel service-owned work. It does not depend on an attached client.
    ///
    /// # Errors
    /// Returns missing task, conflicting terminal state, or persistence errors.
    pub async fn cancel(&self, id: &str) -> Result<TaskView> {
        let service = self.clone();
        let id = id.to_owned();
        self.own_command(async move { service.cancel_owned(&id).await })
            .await
    }

    async fn cancel_owned(&self, id: &str) -> Result<TaskView> {
        let mut state = self.inner.state.lock().await;
        if state.poisoned {
            return Err(ApiError::storage());
        }
        if state.closing {
            return Err(ApiError::new("capacity", "service is shutting down"));
        }
        let entry = state.tasks.get_mut(id).ok_or_else(not_found)?;
        if entry.receipt.phase == Phase::AwaitingApproval {
            let mut receipt = entry.receipt.clone();
            receipt.phase = Phase::Cancelled;
            if let Err(error) = self.persist(receipt.clone()).await {
                state.poisoned = true;
                return Err(error);
            }
            let entry = state.tasks.get_mut(id).ok_or_else(not_found)?;
            entry.receipt = receipt;
            entry.changed.send_replace(0);
        } else if entry.receipt.phase.terminal() && entry.receipt.phase != Phase::Cancelled {
            return Err(ApiError::new("conflict", "task is already terminal"));
        } else {
            entry.cancellation.cancel();
        }
        let receipt = state.tasks.get(id).ok_or_else(not_found)?.receipt.clone();
        drop(state);
        self.view(receipt).await
    }

    /// Return a bounded task list. No execution capability is used during inspection.
    ///
    /// # Errors
    /// Returns record read or corruption errors.
    pub async fn list(&self) -> Result<Vec<TaskSummary>> {
        let receipts = self
            .inner
            .state
            .lock()
            .await
            .tasks
            .values()
            .map(|e| e.receipt.clone())
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for receipt in receipts {
            let objective = receipt
                .conversation
                .as_ref()
                .and_then(|context| context.messages.last())
                .map_or_else(
                    || {
                        receipt
                            .prepared
                            .as_ref()
                            .map_or("Fixture task", |t| t.objective.as_str())
                    },
                    |message| message.text.as_str(),
                );
            let objective = fishnet_core::protocol::BoundedText::new(objective, 160).text;
            let view = self.view(receipt).await?;
            result.push(TaskSummary {
                task_id: view.task_id,
                request_id: view.request_id,
                phase: view.phase,
                state: view.run.map(|r| r.state),
                objective,
            });
        }
        Ok(result)
    }

    /// # Errors
    /// Returns missing task or journal errors.
    pub async fn snapshot(&self, id: &str) -> Result<TaskView> {
        let receipt = self
            .inner
            .state
            .lock()
            .await
            .tasks
            .get(id)
            .ok_or_else(not_found)?
            .receipt
            .clone();
        self.view(receipt).await
    }

    async fn record(&self, id: String) -> Result<Option<fishnet_core::RunRecord>> {
        let store = self.inner.config.runs.clone();
        tokio::task::spawn_blocking(move || match inspect(&store, &id) {
            Ok(record) => Ok(Some(record)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(ApiError::storage()),
        })
        .await
        .map_err(|_| ApiError::storage())?
    }

    async fn view(&self, receipt: Receipt) -> Result<TaskView> {
        let record = self.record(receipt.task_id.clone()).await?;
        Ok(view(receipt, record.as_ref()))
    }

    /// Page durable events from a cursor. A slow consumer holds no event queue.
    ///
    /// # Errors
    /// Returns missing task, invalid cursor, or journal errors.
    pub async fn events(&self, id: &str, after: u64, wait_ms: u64) -> Result<EventPage> {
        if wait_ms > 15_000 {
            return Err(ApiError::invalid("event wait exceeds 15000 milliseconds"));
        }
        let mut changed = self
            .inner
            .state
            .lock()
            .await
            .tasks
            .get(id)
            .ok_or_else(not_found)?
            .changed
            .subscribe();
        let mut page = self.page(id, after).await?;
        if page.events.is_empty() && !page.gap && !page.snapshot.phase.terminal() && wait_ms > 0 {
            let _ = tokio::time::timeout(Duration::from_millis(wait_ms), changed.changed()).await;
            page = self.page(id, after).await?;
        }
        Ok(page)
    }

    async fn page(&self, id: &str, after: u64) -> Result<EventPage> {
        let receipt = self
            .inner
            .state
            .lock()
            .await
            .tasks
            .get(id)
            .ok_or_else(not_found)?
            .receipt
            .clone();
        let record = self.record(id.into()).await?;
        let snapshot = view(receipt, record.as_ref());
        let newest = snapshot.cursor;
        if after > newest {
            return Err(ApiError::new(
                "cursor_ahead",
                "cursor is ahead of this journal; recover from the snapshot",
            ));
        }
        let retained_from = newest.saturating_sub(RETAINED_EVENTS as u64 - 1).max(1);
        let gap = after.saturating_add(1) < retained_from;
        let events = if gap {
            Vec::new()
        } else {
            record
                .map(|r| {
                    r.events
                        .into_iter()
                        .filter(|e| e.sequence > after)
                        .take(PAGE_EVENTS)
                        .collect()
                })
                .unwrap_or_default()
        };
        let cursor = events.last().map_or(after, |e: &Event| e.sequence);
        Ok(EventPage {
            snapshot,
            events,
            cursor,
            retained_from,
            gap,
        })
    }

    async fn commit_and_schedule(
        &self,
        state: &mut State,
        receipt: Receipt,
        permit: Option<OwnedSemaphorePermit>,
    ) -> Result<()> {
        // Reserve execution capacity before committing acceptance. Transfer it only after durability.
        if let Err(error) = self.persist(receipt.clone()).await {
            state.poisoned = true;
            return Err(error);
        }
        if let Some(entry) = state.tasks.get_mut(&receipt.task_id) {
            entry.receipt = receipt.clone();
            entry.changed.send_replace(0);
        } else {
            state.tasks.insert(
                receipt.task_id.clone(),
                Managed {
                    pending_since: if receipt.phase == Phase::AwaitingApproval {
                        Some(std::time::Instant::now())
                    } else {
                        None
                    },
                    receipt: receipt.clone(),
                    cancellation: Cancellation::default(),
                    changed: watch::channel(0).0,
                },
            );
        }
        self.launch_if_ready(receipt, permit);
        Ok(())
    }

    fn launch_if_ready(&self, receipt: Receipt, permit: Option<OwnedSemaphorePermit>) {
        if let Some(permit) = permit {
            self.launch(receipt, permit);
        }
    }

    async fn own_command<T: Send + 'static>(
        &self,
        work: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        let permit = self
            .inner
            .commands
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError::new("capacity", "too many service commands"))?;
        tokio::spawn(async move {
            let _permit = permit;
            work.await
        })
        .await
        .map_err(|_| ApiError::storage())?
    }

    fn launch(&self, receipt: Receipt, permit: OwnedSemaphorePermit) {
        let service = self.clone();
        tokio::spawn(async move {
            let _permit = permit;
            service.worker(receipt).await;
        });
    }

    async fn worker(&self, mut receipt: Receipt) {
        let (cancel, changed) = {
            let state = self.inner.state.lock().await;
            let Some(entry) = state.tasks.get(&receipt.task_id) else {
                return;
            };
            let owned = (entry.cancellation.clone(), entry.changed.clone());
            drop(state);
            owned
        };
        receipt.phase = Phase::Executing;
        if self.update(receipt.clone()).await.is_err() {
            return;
        }
        let notify = changed.clone();
        let result =
            execution::execute(&self.inner.config, &receipt, cancel.clone(), move |event| {
                notify.send_replace(event.sequence);
            })
            .await;
        receipt.phase = if result.is_ok() {
            Phase::Finished
        } else if cancel.is_cancelled() {
            Phase::Cancelled
        } else if result
            .as_ref()
            .is_err_and(|error| matches!(error.code.as_str(), "invalid_request" | "provider_error"))
        {
            // Provider initialization failed before the core could start a model/tool loop.
            Phase::Failed
        } else {
            Phase::Interrupted
        };
        receipt.message = result.err().map(|e| e.to_string());
        let _ = self.update(receipt).await;
        changed.send_replace(u64::MAX);
    }

    async fn update(&self, receipt: Receipt) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        if let Err(error) = self.persist(receipt.clone()).await {
            state.poisoned = true;
            return Err(error);
        }
        let id = receipt.task_id.clone();
        state.tasks.get_mut(&id).ok_or_else(not_found)?.receipt = receipt;
        drop(state);
        Ok(())
    }

    /// Return the explicit host shutdown signal. Client detachment does not set it.
    #[must_use]
    pub fn shutdown_signal(&self) -> Cancellation {
        self.inner.stop.clone()
    }

    /// Cancel active tasks and wait for their owned cleanup before closing the service.
    pub async fn shutdown(&self) {
        {
            let mut state = self.inner.state.lock().await;
            state.closing = true;
            for entry in state.tasks.values() {
                if !entry.receipt.phase.terminal() {
                    entry.cancellation.cancel();
                }
            }
        }
        let _ = self.inner.commands.clone().acquire_many_owned(16).await;
        let _ = self.inner.slots.clone().acquire_many_owned(4).await;
    }
}

fn not_found() -> ApiError {
    ApiError::new("not_found", "task is not in this service store")
}

fn view(receipt: Receipt, record: Option<&fishnet_core::RunRecord>) -> TaskView {
    let approval = conversation::approval(&receipt);
    TaskView {
        task_id: receipt.task_id,
        request_id: receipt.submission.request_id,
        phase: receipt.phase,
        message: receipt.message,
        approval,
        approval_decision: receipt.decision,
        approval_wait_ms: receipt.approval_wait_ms,
        cursor: record
            .and_then(|r| r.events.last())
            .map_or(0, |e| e.sequence),
        run: record.map(|r| RunView {
            state: r.state,
            outcome: r.outcome.clone(),
            incomplete: r.incomplete,
            partial_tail: r.partial_tail,
            pending_operation_ids: r.pending_operation_ids.clone(),
            statistics: r.statistics.clone(),
        }),
    }
}

fn rooted(root: &std::path::Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests;
