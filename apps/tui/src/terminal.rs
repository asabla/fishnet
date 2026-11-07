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
    if let Some((path, app)) = parse(args)? {
        run_terminal(path, app)?;
    }
    Ok(())
}

fn parse(mut args: impl Iterator<Item = String>) -> TerminalResult<Option<(PathBuf, App)>> {
    let mut path = PathBuf::from(".fishnet/service/connection.json");
    let mut app = App::default();
    let mut task = None;
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            println!(
                "fishnet-tui: terminal service client\nUsage: fishnet-tui [--connection PATH] [--attach TASK_ID] [--task tasks/FILE.json]\n                   [--model NAME] [--account LABEL]\nStart make daemon in another terminal. Use --allow-code on that daemon for coding.\nKeys: n offline fixture, p coding task, a approval review, c cancel, ? help, q detach.\nCtrl-C detaches. Accepted work continues in the service."
            );
            return Ok(None);
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--connection" => path = value.into(),
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
    Ok(Some((path, app)))
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
    let _restore = Restore;
    execute!(io::stdout(), EnableBracketedPaste)?;
    let mut worker = Worker::start(path)?;
    if let Some(id) = app.selected.clone() {
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
            terminal.draw(|frame| crate::render::draw(frame, &app))?;
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
    Ok(())
}
fn dispatch(app: &mut App, worker: &Worker, action: Action) {
    if worker.actions.try_send(action).is_err() {
        app.update(Message::Error(
            "Client command queue is full. No new command was accepted; wait and retry explicitly."
                .into(),
        ));
    }
}
