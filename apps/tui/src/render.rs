mod prompt;
mod theme;
mod transcript;
mod wordmark;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

use crate::app::{App, Modal, NoticeLevel, safe, status};

const HELP: &str = "Writing\n\nType a message. Enter sends it after initial provider consent. Conversation history is retained locally and sent with later messages. Chat cannot read files or run commands. Responses are unverified.\n\nUse /code for an explicit code task with file selection and a host grant. Each code task is separate.\n\nEnter          Send message or run command\nAlt/Shift-Enter or Ctrl-J  New line\nLeft/Right     Move the text cursor\nHome/End       Start or end of prompt\nCtrl-U         Clear prompt\nTab            Complete a unique slash command\nCtrl-P         Open saved tasks\nPageUp/PageDown Scroll conversation\nEsc            Follow latest output; close dialog\nF1 / F2        Help / details\n\nCommands\n\n/new /chat  Start a new conversation\n/code       Prepare an explicit code task\n/tasks      Open saved tasks\n/review     Review pending consent or access\n/cancel     Request cancellation confirmation\n/details    Diagnostics and recent event output\n/fixture    Offline tool example\n/taskfile   Advanced task contract\n/refresh    Refresh connection\n/quit       Detach after acknowledgement\n\nConsent\n\nEnter or y approves; n rejects; F2 toggles full details.\nOnly independent checks establish verified success.\n\nLeaving\n\nCtrl-C detaches. Accepted work continues.\nUse fishnet stop outside the TUI to stop the service.\nCancellation does not roll back completed edits.";

/// Draw a conversation and a persistent prompt. The service owns task state.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    draw_content(frame, app);
    if app.appearance == crate::appearance::Appearance::Monochrome {
        // NO_COLOR removes backgrounds too. Reverse video keeps inputs and tool
        // blocks visible using the terminal's own foreground and background.
        for cell in &mut frame.buffer_mut().content {
            if matches!(cell.bg, theme::SURFACE | Color::Rgb(44, 44, 44)) {
                cell.modifier.toggle(Modifier::REVERSED);
            }
            cell.fg = Color::Reset;
            cell.bg = Color::Reset;
        }
    }
}

fn draw_content(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme::base()), area);
    if area.width < 40 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("fishnet\nUse at least 40 columns and 12 rows.\nCtrl-C detaches.")
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let home = app.snapshot.is_none() && app.selected.is_none() && app.transcript.is_empty();
    let attention = app.notice_level != NoticeLevel::Info || !app.connected;
    let [main, alert, state] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(u16::from(attention)),
        Constraint::Length(1),
    ])
    .areas(area);
    if home {
        home_screen(frame, app, main);
    } else {
        session_screen(frame, app, main);
    }
    footer(frame, app, if home { state } else { column(state, 100) });
    if attention {
        frame.render_widget(
            Paragraph::new(elide(
                &app.notice,
                usize::from(alert.width.saturating_sub(2)),
            ))
            .style(theme::base().fg(Color::Yellow)),
            Rect {
                x: alert.x + 1,
                width: alert.width.saturating_sub(2),
                ..alert
            },
        );
    }
    if app.show_tasks {
        let inset = dialog_area(area, 24);
        frame.render_widget(Clear, inset);
        task_list(frame, app, inset);
    }
    if app.modal.is_some() {
        normalize_dialog(app, area);
        if let Some(modal) = &app.modal {
            dialog(frame, modal, area);
        }
    }
}

fn column(area: Rect, max_width: u16) -> Rect {
    let width = area.width.saturating_sub(4).min(max_width);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y,
        width,
        area.height,
    )
}

fn home_screen(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let column = column(area, 80);
    let height = prompt::height(app, column.width, area.height);
    let logo_height = if column.width >= 42 { 3 } else { 1 };
    let total = logo_height + 2 + height + 1;
    let top = column.y + column.height.saturating_sub(total) / 2;
    wordmark::draw(frame, Rect::new(column.x, top, column.width, logo_height));
    let prompt = Rect::new(column.x, top + logo_height + 2, column.width, height);
    prompt::draw(frame, app, prompt);
    context_line(
        frame,
        app,
        Rect::new(column.x, prompt.bottom(), column.width, 1),
    );
    prompt::command_menu(frame, app, prompt);
}

fn session_screen(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let column = column(area, 100);
    let height = prompt::height(app, column.width, area.height);
    let [heading, body, prompt, context] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(height),
        Constraint::Length(1),
    ])
    .areas(column);
    let title = app
        .tasks
        .iter()
        .find(|task| Some(&task.task_id) == app.selected.as_ref())
        .map_or("fishnet", |task| task.objective.as_str());
    frame.render_widget(
        Paragraph::new(elide(title, usize::from(heading.width)))
            .style(theme::base().add_modifier(Modifier::BOLD)),
        heading,
    );
    normalize_scroll(app, body);
    transcript::draw(frame, app, body);
    prompt::draw(frame, app, prompt);
    context_line(frame, app, context);
    prompt::command_menu(frame, app, prompt);
}

fn context_line(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let keys = if area.width < 60 {
        "/ commands · ctrl+p tasks"
    } else {
        "/ commands   ctrl+p tasks   F1 help"
    };
    let available = usize::from(area.width).saturating_sub(keys.len() + 2);
    let project = elide(&app.project, available);
    frame.render_widget(Paragraph::new(project).style(theme::muted()), area);
    frame.render_widget(
        Paragraph::new(keys)
            .alignment(ratatui::layout::Alignment::Right)
            .style(theme::muted()),
        area,
    );
}

fn footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let version = concat!("fishnet ", env!("CARGO_PKG_VERSION"));
    let width = if area.width >= 80 && app.uncertain.is_empty() {
        u16::try_from(version.len()).unwrap_or(0)
    } else {
        0
    };
    let [state, version_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(width)]).areas(area);
    frame.render_widget(Paragraph::new(caption(app, state.width)), state);
    if width > 0 {
        frame.render_widget(Paragraph::new(version).style(theme::muted()), version_area);
    }
}

fn normalize_scroll(app: &mut App, body: Rect) {
    let rows = transcript::rows(app, body.width);
    let max = rows.saturating_sub(usize::from(body.height));
    app.scroll = app.scroll.min(u16::try_from(max).unwrap_or(u16::MAX));
}

fn normalize_dialog(app: &mut App, area: Rect) {
    let Some(modal) = &mut app.modal else {
        return;
    };
    let compact = matches!(
        modal,
        Modal::Review {
            expanded: false,
            ..
        }
    );
    let (text, scroll) = match modal {
        Modal::Review {
            text,
            summary,
            expanded,
            scroll,
            ..
        } => (
            if *expanded {
                text.as_str()
            } else {
                summary.as_str()
            },
            scroll,
        ),
        Modal::Notice { text, scroll } => (text.as_str(), scroll),
        Modal::Help { scroll } => (HELP, scroll),
        _ => return,
    };
    let dialog = dialog_area(area, if compact { 14 } else { 28 });
    let inner = modal_block("").inner(dialog);
    let rows = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .line_count(inner.width);
    let max = rows.saturating_sub(usize::from(inner.height.saturating_sub(2)));
    *scroll = (*scroll).min(u16::try_from(max).unwrap_or(u16::MAX));
}

fn caption(app: &App, width: u16) -> Line<'static> {
    let connection = if app.connected {
        ""
    } else {
        " · Disconnected"
    };
    let label = app.snapshot.as_ref().map_or("Ready", |view| {
        status(
            view.phase,
            view.run.as_ref().map(|run| run.state),
            view.run.as_ref().is_some_and(|run| run.incomplete),
        )
    });
    let hint = if app
        .snapshot
        .as_ref()
        .is_some_and(|view| view.approval.is_some())
    {
        " · /review"
    } else if app.scroll > 0 {
        " · Esc latest"
    } else if label == "RUNNING" && app.conversation.is_some() {
        " · responding"
    } else if label == "RUNNING" && width >= 60 {
        " · checks still required"
    } else {
        ""
    };
    let uncertain = if app.uncertain.is_empty() {
        String::new()
    } else {
        format!(" · {} uncertain · F2 details", app.uncertain.len())
    };
    let appearance = if app.appearance == crate::appearance::Appearance::Monochrome {
        " · monochrome · --color always"
    } else {
        ""
    };
    Line::from(vec![
        Span::styled(format!(" {label}"), state_style(label)),
        Span::styled(
            elide(
                &format!("{connection}{hint}{uncertain}{appearance}"),
                usize::from(width).saturating_sub(label.len() + 1),
            ),
            theme::muted(),
        ),
    ])
}

fn elide(text: &str, width: usize) -> String {
    let text = safe(text, 512);
    if Line::from(text.as_str()).width() <= width {
        return text;
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    for ch in text.chars() {
        result.push(ch);
        if Line::from(result.as_str()).width() >= width {
            result.pop();
            break;
        }
    }
    result.push('…');
    result
}

fn task_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = modal_block("Saved tasks").style(theme::surface());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [body, keys] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new("↑↓ Select  Enter Open  Esc Close").style(theme::muted().bg(theme::SURFACE)),
        keys,
    );
    if app.tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("No saved tasks.").style(theme::surface()),
            body,
        );
        return;
    }
    let items = app
        .tasks
        .iter()
        .map(|task| {
            let label = status(task.phase, task.state, false);
            ListItem::new(vec![
                Line::styled(
                    elide(&task.objective, usize::from(body.width.saturating_sub(2))),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Line::styled(label, state_style(label)),
                Line::styled(
                    short(&task.task_id),
                    Style::default().add_modifier(Modifier::DIM),
                ),
                Line::from(""),
            ])
        })
        .collect::<Vec<_>>();
    let selected = app
        .tasks
        .iter()
        .position(|task| Some(&task.task_id) == app.selected.as_ref());
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .style(theme::surface())
            .highlight_style(theme::surface().bg(Color::Rgb(44, 44, 44)))
            .highlight_symbol("> "),
        body,
        &mut state,
    );
}

fn state_style(label: &str) -> Style {
    if label == "Ready" {
        return theme::muted();
    }
    let color = if label == "VERIFIED SUCCESS" {
        Color::Green
    } else if matches!(label, "FAILED" | "REJECTED") {
        Color::Red
    } else if matches!(label, "RUNNING" | "QUEUED") {
        theme::ACCENT
    } else {
        Color::Yellow
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

fn dialog(frame: &mut Frame<'_>, modal: &Modal, area: Rect) {
    let height = match modal {
        Modal::Scope { .. } | Modal::Code { .. } => 12,
        Modal::Cancel(_) => 10,
        Modal::Review {
            expanded: false, ..
        } => 14,
        _ => 28,
    };
    let inset = dialog_area(area, height);
    frame.render_widget(Clear, inset);
    frame.render_widget(Block::default().style(theme::surface()), inset);
    match modal {
        Modal::Scope {
            objective,
            files,
            cursor,
        } => scope_form(frame, objective, files, *cursor, inset),
        Modal::Code {
            fields,
            focus,
            cursor,
        } => code_form(frame, fields, *focus, *cursor, inset),
        Modal::Review {
            text,
            summary,
            expanded,
            scroll,
            ..
        } => text_dialog(
            frame,
            if *expanded {
                "Full grant details"
            } else {
                "Allow provider access?"
            },
            if *expanded { text } else { summary },
            *scroll,
            "Enter Allow  n No  F2 More  Esc",
            inset,
        ),
        Modal::Cancel(id) => text_dialog(
            frame,
            "Cancel task",
            &format!(
                "Cancel task {}?\n\nCompleted edits and partial command effects can remain.\nCancellation does not roll them back.",
                short(id)
            ),
            0,
            "y Cancel task  ·  Other key: close",
            inset,
        ),
        Modal::Notice { text, scroll } => text_dialog(
            frame,
            "Task details and recent status",
            text,
            *scroll,
            "Up/Down Scroll  Esc Close",
            inset,
        ),
        Modal::Help { scroll } => text_dialog(
            frame,
            "Help",
            HELP,
            *scroll,
            "Up/Down Scroll  Esc Close",
            inset,
        ),
    }
}
fn dialog_area(area: Rect, max_height: u16) -> Rect {
    let column = column(area, 100);
    let height = area.height.saturating_sub(2).min(max_height);
    Rect::new(
        column.x,
        area.y + area.height.saturating_sub(height) / 2,
        column.width,
        height,
    )
}

fn modal_block(title: &str) -> Block<'_> {
    Block::default()
        .style(theme::surface())
        .borders(Borders::TOP)
        .border_style(theme::muted().bg(theme::SURFACE))
        .padding(Padding::horizontal(2))
        .title(title)
}

fn text_dialog(
    frame: &mut Frame<'_>,
    title: &str,
    text: &str,
    scroll: u16,
    hint: &str,
    area: Rect,
) {
    let block = modal_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    let paragraph = Paragraph::new(text)
        .style(theme::surface())
        .wrap(Wrap { trim: false });
    let rows = paragraph.line_count(body.width);
    let offset = usize::from(scroll).min(rows.saturating_sub(usize::from(body.height)));
    frame.render_widget(
        paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        body,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{hint}\n{}",
            if rows > usize::from(body.height) {
                format!(
                    "Rows {}–{} of {rows}",
                    offset + 1,
                    (offset + usize::from(body.height)).min(rows)
                )
            } else {
                String::new()
            }
        ))
        .wrap(Wrap { trim: false })
        .style(theme::surface().add_modifier(Modifier::DIM)),
        footer,
    );
}

fn scope_form(frame: &mut Frame<'_>, objective: &str, files: &str, cursor: usize, area: Rect) {
    let block = modal_block("File access");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intent, input, explanation, footer] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(objective)
            .style(theme::surface())
            .wrap(Wrap { trim: false }),
        intent,
    );
    field(
        frame,
        "Files to change (comma-separated)",
        files,
        true,
        cursor,
        input,
    );
    frame.render_widget(
        Paragraph::new("Blank: no source edits.\nNamed files must change.")
            .wrap(Wrap { trim: false })
            .style(theme::muted().bg(theme::SURFACE)),
        explanation,
    );
    frame.render_widget(
        Paragraph::new("Enter Review grant · Esc Edit").style(theme::surface()),
        footer,
    );
}

fn code_form(frame: &mut Frame<'_>, fields: &[String; 3], focus: usize, cursor: usize, area: Rect) {
    let block = modal_block("Advanced task file");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intro, path, model, account, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new("Use a reviewed file under tasks/."), intro);
    for (index, ((label, value), field_area)) in
        ["Task file", "Model (blank = account default)", "Account"]
            .into_iter()
            .zip(fields)
            .zip([path, model, account])
            .enumerate()
    {
        field(
            frame,
            label,
            value,
            index == focus,
            if index == focus { cursor } else { value.len() },
            field_area,
        );
    }
    frame.render_widget(
        Paragraph::new("Tab Next  Enter Review  Esc Close").wrap(Wrap { trim: false }),
        footer,
    );
}
fn field(frame: &mut Frame<'_>, label: &str, value: &str, active: bool, cursor: usize, area: Rect) {
    let [label_area, input] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let style = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };
    frame.render_widget(Paragraph::new(label).style(style), label_area);
    let before = safe(value.get(..cursor).unwrap_or(value), 512);
    let text = safe(value, 512);
    if input.height > 1 {
        textarea(frame, &text, &before, active, input);
        return;
    }
    let column = Line::from(before).width();
    let width = usize::from(input.width.saturating_sub(1));
    let offset = column.saturating_sub(width);
    frame.render_widget(
        Paragraph::new(text)
            .style(theme::surface())
            .scroll((0, u16::try_from(offset).unwrap_or(u16::MAX))),
        input,
    );
    if active && input.width > 0 && input.height > 0 {
        frame.set_cursor_position((
            input
                .x
                .saturating_add(u16::try_from(column.min(width)).unwrap_or(0)),
            input.y,
        ));
    }
}
fn textarea(frame: &mut Frame<'_>, text: &str, before: &str, active: bool, area: Rect) {
    let tail = text.get(before.len()..).unwrap_or_default();
    let paragraph = if active {
        Paragraph::new(Line::from(vec![
            Span::raw(before),
            Span::styled(
                "▏",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(tail),
        ]))
    } else {
        Paragraph::new(text)
    }
    .wrap(Wrap { trim: false });
    let prefix = Paragraph::new(format!("{before}▏")).wrap(Wrap { trim: false });
    let offset = prefix
        .line_count(area.width)
        .saturating_sub(usize::from(area.height));
    frame.render_widget(
        paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        area,
    );
}
