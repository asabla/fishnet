use std::path::PathBuf;
use std::time::{Duration, Instant};

use fishnet_core::Cancellation;
use fishnet_service::{
    client::Client,
    contract::{ApiError, Result},
};
use tokio::sync::mpsc;

use crate::app::{Action, Message};

/// Bounded client I/O on a separate thread. Dropping it detaches without task cancellation.
pub struct Worker {
    pub actions: mpsc::Sender<Action>,
    pub messages: mpsc::Receiver<Message>,
    cancel: Cancellation,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker").finish_non_exhaustive()
    }
}
impl Worker {
    /// # Errors
    /// Returns a thread startup error. Connection errors are delivered as UI observations.
    pub fn start(connection: PathBuf) -> std::io::Result<Self> {
        let (actions, commands) = mpsc::channel(8);
        let (updates, messages) = mpsc::channel(8);
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        let thread = std::thread::Builder::new()
            .name("fishnet-tui-client".into())
            .spawn(move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    runtime.block_on(async move {
                        tokio::select! {
                            ()=signal.cancelled()=>(),
                            ()=run(connection,commands,updates)=>(),
                        }
                    });
                    // Client detachment must not wait indefinitely for a blocking DNS helper.
                    runtime.shutdown_timeout(Duration::from_secs(1));
                }
            })?;
        Ok(Self {
            actions,
            messages,
            cancel,
            thread: Some(thread),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Feed {
    selected: Option<String>,
    cursor: u64,
    last_list: Option<Instant>,
    tasks: Option<Vec<fishnet_service::contract::TaskSummary>>,
    snapshot: Option<(fishnet_service::contract::Phase, u64, Option<String>)>,
    connected: bool,
    error: Option<String>,
}
async fn run(path: PathBuf, mut commands: mpsc::Receiver<Action>, updates: mpsc::Sender<Message>) {
    let mut feed = Feed {
        selected: None,
        cursor: 0,
        last_list: None,
        tasks: None,
        snapshot: None,
        connected: false,
        error: None,
    };
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            action=commands.recv()=>{
                let Some(action)=action else {return;};
                match action {
                    Action::Select(id)=>{feed.selected=Some(id);feed.cursor=0;feed.snapshot=None;}
                    Action::Refresh=>feed.last_list=None,
                    action=>{if execute(&path,action,&updates).await.is_err(){return;}}
                }
            }
            _=tick.tick()=>{
                match refresh(&path,&mut feed,&updates).await {
                    Ok(())=>(),Err(error)=>{
                        let detail=format!("Service unavailable. Use /refresh to reconnect; check --connection or run fishnet serve --allow-code to diagnose startup. {error}");
                        if feed.error.as_ref()!=Some(&detail) {
                            if updates.send(Message::Connection {connected:false,detail:detail.clone()}).await.is_err(){return;}
                            feed.error=Some(detail);
                        }
                        feed.connected=false;
                        tokio::time::sleep(Duration::from_millis(750)).await;
                    }
                }
            }
        }
    }
}

async fn refresh(
    path: &std::path::Path,
    feed: &mut Feed,
    updates: &mpsc::Sender<Message>,
) -> Result<()> {
    let client = Client::connect(path)?;
    if feed
        .last_list
        .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
    {
        let tasks = client.list().await?;
        connected(feed, updates).await?;
        feed.last_list = Some(Instant::now());
        if feed.tasks.as_ref() != Some(&tasks) {
            send(updates, Message::Tasks(tasks.clone())).await?;
            feed.tasks = Some(tasks);
        }
    }
    if let Some(id) = &feed.selected {
        match client.events(id, feed.cursor, 0).await {
            Ok(page) => {
                connected(feed, updates).await?;
                let cursor = if page.gap {
                    page.retained_from.saturating_sub(1)
                } else {
                    page.cursor
                };
                let signature = (
                    page.snapshot.phase,
                    page.snapshot.cursor,
                    page.snapshot.message.clone(),
                );
                if !page.events.is_empty() || page.gap || feed.snapshot.as_ref() != Some(&signature)
                {
                    send(updates, Message::Page(page)).await?;
                    feed.snapshot = Some(signature);
                }
                feed.cursor = cursor;
            }
            Err(error) if error.code == "cursor_ahead" => {
                feed.cursor = 0;
                send(
                    updates,
                    Message::Error("Service cursor changed. Recovering from its snapshot.".into()),
                )
                .await?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

async fn connected(feed: &mut Feed, updates: &mpsc::Sender<Message>) -> Result<()> {
    if !feed.connected {
        send(
            updates,
            Message::Connection {
                connected: true,
                detail: "Connected to the task service. Use /quit to detach without cancellation."
                    .into(),
            },
        )
        .await?;
    }
    feed.connected = true;
    feed.error = None;
    Ok(())
}

async fn execute(
    path: &std::path::Path,
    action: Action,
    updates: &mpsc::Sender<Message>,
) -> Result<()> {
    let reference = match &action {
        Action::Submit(request) => format!("request {}", request.request_id),
        Action::Approve { id, .. } | Action::Cancel(id) | Action::Select(id) => {
            format!("task {id}")
        }
        Action::Refresh => "refresh".into(),
    };
    let result = command(path, action).await;
    let message = match result {
        Ok(message) => message,
        Err(error) if matches!(error.code.as_str(), "disconnected" | "storage_error") => {
            Message::CommandError {
                reference: reference.clone(),
                uncertain: true,
                error: format!(
                    "Outcome unknown for {reference}: {error}. Reconnect and inspect this identity. No command was retried."
                ),
            }
        }
        Err(error) => Message::CommandError {
            reference: reference.clone(),
            uncertain: false,
            error: format!("{reference}: {error}"),
        },
    };
    send(updates, message).await
}
async fn command(path: &std::path::Path, action: Action) -> Result<Message> {
    let client = Client::connect(path).map_err(|e| ApiError::new("not_sent", &e.message))?;
    match action {
        Action::Submit(request) => Ok(Message::Submitted(client.submit(request).await?)),
        Action::Approve { id, decision } => {
            Ok(Message::Updated(client.approve(&id, decision).await?))
        }
        Action::Cancel(id) => Ok(Message::Updated(client.cancel(&id).await?)),
        Action::Select(_) | Action::Refresh => Err(ApiError::invalid("unexpected client command")),
    }
}
async fn send(updates: &mpsc::Sender<Message>, message: Message) -> Result<()> {
    updates
        .send(message)
        .await
        .map_err(|_| ApiError::new("detached", "terminal client detached"))
}
