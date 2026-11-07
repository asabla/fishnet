use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, App, ExitIntent, Modal, input};

pub const COMMANDS: [(&str, &str); 10] = [
    ("/new", "Start a new task"),
    ("/tasks", "Open saved tasks"),
    ("/review", "Review pending file and command access"),
    ("/cancel", "Cancel the selected task"),
    ("/details", "Show diagnostics and recent status"),
    ("/fixture", "Run the offline example"),
    ("/taskfile", "Open an advanced task file"),
    ("/refresh", "Refresh the service connection"),
    ("/help", "Show keys and limits"),
    ("/quit", "Detach; accepted work continues"),
];

/// The prompt owns its text and cursor. Paste never submits a command.
#[derive(Clone, Debug, Default)]
pub struct Composer {
    pub text: String,
    pub cursor: usize,
}
impl Composer {
    pub fn paste(&mut self, text: &str) {
        input::insert(&mut self.text, &mut self.cursor, text, 4096, true);
    }

    pub fn edit(&mut self, key: KeyEvent) {
        input::edit_text(&mut self.text, &mut self.cursor, key, 4096);
    }
}

impl App {
    pub(super) fn prompt_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Enter
                if key
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
            {
                self.composer.paste("\n");
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.composer.paste("\n");
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_tasks = true;
            }
            KeyCode::F(1) => self.modal = Some(Modal::Help { scroll: 0 }),
            KeyCode::F(2) => self.open_details(),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Esc => self.scroll = 0,
            KeyCode::Tab => self.complete_command(),
            KeyCode::Enter => return self.send_prompt(),
            _ => self.composer.edit(key),
        }
        None
    }

    fn send_prompt(&mut self) -> Option<Action> {
        let text = self.composer.text.trim().to_owned();
        if text.is_empty() {
            return None;
        }
        if text.starts_with('/') {
            return self.command(&text);
        }
        if !self.pending.is_empty() {
            self.set_notice(
                "A command still awaits acknowledgement. Your prompt is retained.".into(),
            );
            return None;
        }
        // The service still freezes an exact file grant before it calls a provider.
        // Typing intent is not permission to write arbitrary workspace files.
        self.modal = Some(Modal::Scope {
            objective: text,
            files: String::new(),
            cursor: 0,
        });
        None
    }

    fn command(&mut self, command: &str) -> Option<Action> {
        if !COMMANDS.iter().any(|(name, _)| *name == command) {
            self.set_notice("Unknown command. Type / to see commands, or use /help.".into());
            return None;
        }
        self.composer = Composer::default();
        match command {
            "/new" => {
                self.selected = None;
                self.snapshot = None;
                self.lines.clear();
                self.transcript.clear();
                self.scroll = 0;
                self.show_tasks = false;
            }
            "/tasks" => self.show_tasks = true,
            "/review" => self.open_review(),
            "/cancel" => {
                if let Some(id) = &self.selected {
                    self.modal = Some(Modal::Cancel(id.clone()));
                }
            }
            "/details" => self.open_details(),
            "/fixture" => return self.fixture(),
            "/taskfile" => self.open_code(String::new()),
            "/refresh" => return Some(Action::Refresh),
            "/help" => self.modal = Some(Modal::Help { scroll: 0 }),
            "/quit" => self.detach(),
            _ => (),
        }
        None
    }

    fn complete_command(&mut self) {
        if !self.composer.text.starts_with('/') {
            return;
        }
        let mut matches = COMMANDS
            .iter()
            .filter(|(name, _)| name.starts_with(&self.composer.text));
        if let Some((name, _)) = matches.next()
            && matches.next().is_none()
        {
            self.composer.text = (*name).into();
            self.composer.cursor = name.len();
        }
    }

    fn detach(&mut self) {
        if self.pending.is_empty() {
            self.quit = true;
        } else {
            self.exit_intent = ExitIntent::WaitingForCommands;
            self.set_notice("Waiting for command acknowledgements before detaching. Ctrl-C detaches immediately; Esc returns to the task.".into());
        }
    }

    fn open_details(&mut self) {
        let mut text = self.details();
        text.push_str(&self.notice);
        for previous in self
            .notices
            .iter()
            .rev()
            .filter(|message| *message != &self.notice)
        {
            text.push_str("\n\n");
            text.push_str(previous);
        }
        text.push_str("\n\nRecent event details\n");
        for line in &self.lines {
            text.push_str(line);
            text.push('\n');
        }
        self.modal = Some(Modal::Notice { text, scroll: 0 });
    }
}
