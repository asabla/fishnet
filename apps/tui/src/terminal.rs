use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
};

use crate::{
    app::{Action, App, Message},
    worker::Worker,
};

type TerminalResult<T> = Result<T, Box<dyn std::error::Error>>;
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        let _ = ratatui::try_restore();
    }
}

/// Launch a terminal client. Detaching never cancels service-owned work.
///
/// # Errors
/// Returns invalid options, terminal setup, worker startup, or rendering errors.
pub fn launch(args: impl Iterator<Item = String>) -> TerminalResult<()> {
    if let Some((path, mut app, start)) = parse(args)? {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("fishnet requires an interactive terminal; use fishnet --help or fishnet tasks for headless access".into());
        }
        if start {
            let started = fishnet_service::local::ensure(&std::env::current_exe()?)?;
            app.notice = if started {
                "Local service started. New task: n. Help: ?. Detach: q."
            } else {
                "Connected to the existing local service. New task: n. Help: ?."
            }
            .into();
        }
        run_terminal(path, app)?;
    }
    Ok(())
}

fn parse(mut args: impl Iterator<Item = String>) -> TerminalResult<Option<(PathBuf, App, bool)>> {
    let mut path = PathBuf::from(".fishnet/service/connection.json");
    let mut app = App::default();
    let mut task = None;
    let mut start = true;
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            println!(
                "fishnet: terminal interface\nUsage: fishnet [--connection PATH] [--attach TASK_ID] [--task tasks/FILE.json]\n               [--model NAME] [--account LABEL] [--no-start]\nThe local service starts automatically. Explicit --connection only attaches.\nKeys: n new Rust task, f offline fixture, p task file, a approval, c cancel, ? help, q detach.\nCtrl-C detaches. Accepted work continues. Use fishnet stop to shut down the service."
            );
            return Ok(None);
        }
        if option == "--no-start" {
            start = false;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--connection" => {
                path = value.into();
                start = false;
            }
            "--attach" => {
                fishnet_service::contract::validate_key(&value)?;
                app.selected = Some(value);
            }
            "--task" => task = Some(value),
            "--model" => app.default_model = value,
            "--account" => app.account = value,
            _ => return Err(format!("unknown TUI option: {option}").into()),
        }
    }
    if let Some(task) = task {
        app.open_code(task);
    }
    Ok(Some((path, app, start)))
}

fn run_terminal(path: PathBuf, mut app: App) -> TerminalResult<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "fishnet-tui requires an interactive terminal; use fishnet task for headless access"
                .into(),
        );
    }
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = ratatui::try_restore();
            return Err(error.into());
        }
    };
    let restore = Restore;
    execute!(io::stdout(), EnableBracketedPaste)?;
    let mut worker = Worker::start(path)?;
    if let Some(id) = app.selected.clone().filter(|id| id.len() == 64) {
        dispatch(&mut app, &worker, Action::Select(id));
    }
    let mut dirty = true;
    while !app.quit {
        for _ in 0..8 {
            match worker.messages.try_recv() {
                Ok(message) => {
                    dirty = true;
                    if let Some(action) = app.update(message) {
                        dispatch(&mut app, &worker, action);
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    return Err("terminal connection worker stopped".into());
                }
            }
        }
        if dirty {
            terminal.draw(|frame| crate::render::draw(frame, &mut app))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(50))? {
            dirty = true;
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = app.key(key) {
                        dispatch(&mut app, &worker, action);
                    }
                }
                Event::Paste(text) => app.paste(&text),
                Event::Resize(_, _) | Event::FocusGained | Event::FocusLost | Event::Mouse(_) => (),
            }
        }
    }
    drop(worker);
    drop(restore);
    if !app.pending.is_empty() || !app.uncertain.is_empty() {
        eprintln!(
            "Detached before service acknowledgement. These command outcomes may be unknown:\n{}\nUse fishnet tasks --json to match request_id or task_id before submitting again.",
            app.pending
                .iter()
                .chain(&app.uncertain)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    Ok(())
}
fn dispatch(app: &mut App, worker: &Worker, action: Action) {
    let reference = match &action {
        Action::Submit(request) => Some(format!("request {}", request.request_id)),
        Action::Approve { id, .. } | Action::Cancel(id) => Some(format!("task {id}")),
        Action::Select(_) | Action::Refresh => None,
    };
    if reference.is_some() && app.pending.len() >= 16 {
        app.update(Message::Error(
            "Too many commands await service acknowledgement. Wait before sending another command."
                .into(),
        ));
        return;
    }
    if worker.actions.try_send(action).is_err() {
        app.update(Message::Error(
            "Client command queue is full. No new command was accepted; wait and retry explicitly."
                .into(),
        ));
    } else if let Some(reference) = reference {
        app.queued(reference);
    }
}
