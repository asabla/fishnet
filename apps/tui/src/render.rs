use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

use crate::app::{App, COMMANDS, EntryKind, Modal, safe, status};

const HELP: &str = "Writing\n\nType a Rust task in the prompt. Enter opens file access selection, then the host grant. No provider call starts before approval. Each submission is a separate task; prior task text is not sent as conversation context.\n\nEnter          Prepare task or run slash command\nAlt/Shift-Enter or Ctrl-J  New line\nLeft/Right     Move the text cursor\nHome/End       Start or end of prompt\nCtrl-U         Clear prompt\nTab            Complete a unique slash command\nCtrl-P         Open saved tasks\nPageUp/PageDown Scroll conversation\nEsc            Follow latest output; close a dialog\nF1 / F2        Help / details\n\nCommands\n\n/new       Start a new task\n/tasks     Open saved tasks\n/review    Review pending access\n/cancel    Request cancellation confirmation\n/details   Diagnostics and recent event output\n/fixture   Offline example; no account needed\n/taskfile  Advanced task contract\n/refresh   Refresh connection\n/help      This help\n/quit      Detach after command acknowledgement\n\nApproval\n\ny approves; n rejects; arrows scroll; Esc closes.\nOnly independent host checks can establish verified success.\n\nLeaving\n\nCtrl-C detaches immediately. Accepted work continues.\nUse fishnet stop outside the TUI to stop the service.\nCancellation can leave completed edits. It does not roll them back.";

/// Draw a conversation and a persistent prompt. The service owns task state.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    if area.width < 40 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("fishnet\nUse at least 40 columns and 12 rows.\nCtrl-C detaches.")
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let prompt_height = prompt_rows(app, area.width.saturating_sub(4))
        .clamp(1, (area.height / 3).saturating_sub(2))
        .saturating_add(2);
    let [heading, body, prompt, state, notice] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(prompt_height),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let body = Rect {
        x: body.x + 2,
        width: body.width.saturating_sub(4),
        ..body
    };
    normalize_scroll(app, body, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" fishnet", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  ·  {}", safe(&app.project, 80)),
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        heading,
    );
    output(frame, app, body);
    composer(frame, app, prompt);
    frame.render_widget(Paragraph::new(caption(app, state.width)), state);
    let hint = if app.notice.is_empty() {
        "Enter send · / commands · Ctrl-P tasks · Ctrl-C detach"
    } else {
        &app.notice
    };
    frame.render_widget(
        Paragraph::new(elide(hint, usize::from(notice.width)))
            .style(Style::default().fg(Color::DarkGray)),
        notice,
    );
    if app.show_tasks {
        let inset = dialog_area(area, 24);
        frame.render_widget(Clear, inset);
        task_list(frame, app, inset);
    }
    if let Some(modal) = &app.modal {
        dialog(frame, modal, area);
    }
}

fn normalize_scroll(app: &mut App, body: Rect, area: Rect) {
    let rows = Paragraph::new(conversation(app))
        .wrap(Wrap { trim: false })
        .line_count(body.width);
    let max = rows.saturating_sub(usize::from(body.height));
    app.scroll = app.scroll.min(u16::try_from(max).unwrap_or(u16::MAX));
    let Some(modal) = &mut app.modal else {
        return;
    };
    let (text, scroll) = match modal {
        Modal::Review { text, scroll, .. } | Modal::Notice { text, scroll } => {
            (text.as_str(), scroll)
        }
        Modal::Help { scroll } => (HELP, scroll),
        _ => return,
    };
    let dialog = dialog_area(area, 28);
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
    } else if label == "RUNNING" && width >= 60 {
        " · checks still required · / commands"
    } else {
        " · / commands"
    };
    Line::from(vec![
        Span::styled(format!(" {label}"), state_style(label)),
        Span::styled(
            elide(
                &format!("{connection}{hint}"),
                usize::from(width).saturating_sub(label.len() + 1),
            ),
            Style::default().fg(Color::DarkGray),
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
    if app.tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("No saved tasks.\n\nEsc returns to the prompt.")
                .block(panel("Tasks"))
                .wrap(Wrap { trim: false }),
            area,
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
                    elide(&task.objective, usize::from(area.width.saturating_sub(2))),
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
            .block(modal_block(
                "Saved tasks · arrows select · Enter open · Esc close",
            ))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> "),
        area,
        &mut state,
    );
}

fn conversation(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for entry in &app.transcript {
        let (prefix, style) = match entry.kind {
            EntryKind::User => (
                "› ",
                Style::default().fg(Color::White).bg(Color::Rgb(42, 42, 42)),
            ),
            EntryKind::Assistant => ("  ", Style::default()),
            EntryKind::Tool => ("· ", Style::default().fg(Color::DarkGray)),
            EntryKind::Check => ("· ", Style::default().fg(Color::Cyan)),
            EntryKind::Error => ("! ", Style::default().fg(Color::Red)),
            EntryKind::Outcome => ("  ", Style::default().add_modifier(Modifier::BOLD)),
        };
        if entry.kind == EntryKind::Assistant {
            lines.push(Line::styled(
                "  fishnet",
                Style::default().fg(Color::DarkGray),
            ));
        }
        for (index, text) in entry.text.lines().enumerate() {
            lines.push(Line::styled(
                format!("{}{text}", if index == 0 { prefix } else { "  " }),
                style,
            ));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn output(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.snapshot.is_none() && app.transcript.is_empty() {
        let welcome = "fishnet\n\nWhat do you want to work on?\n\nDescribe a Rust task below.\n/fixture  Try the offline example\n/tasks    Open saved tasks\n\nFile and command access is reviewed before execution.";
        let [_, welcome_area, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Fill(2),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(welcome).wrap(Wrap { trim: false }),
            welcome_area,
        );
        return;
    }
    let mut lines = conversation(app);
    if lines.is_empty() {
        if let Some(task) = app
            .tasks
            .iter()
            .find(|task| Some(&task.task_id) == app.selected.as_ref())
        {
            lines.push(Line::styled(
                format!("› {}", safe(&task.objective, 4096)),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(""));
        }
        let text = app
            .snapshot
            .as_ref()
            .and_then(|view| view.message.as_deref())
            .unwrap_or("Waiting for task events. Use /review for pending access.");
        lines.extend(text.lines().map(|line| Line::from(safe(line, 2048))));
    }
    let paragraph = Paragraph::new(lines.clone()).wrap(Wrap { trim: false });
    let last = paragraph
        .line_count(area.width)
        .saturating_sub(usize::from(area.height));
    let offset = last.saturating_sub(usize::from(app.scroll));
    message_backgrounds(frame, &lines, area, offset);
    frame.render_widget(
        paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        area,
    );
}

fn message_backgrounds(frame: &mut Frame<'_>, lines: &[Line<'_>], area: Rect, offset: usize) {
    let mut row: usize = 0;
    for line in lines {
        let rows = Paragraph::new(line.clone())
            .wrap(Wrap { trim: false })
            .line_count(area.width);
        let start = row.max(offset);
        let end = (row + rows).min(offset + usize::from(area.height));
        if let Some(bg) = line.style.bg
            && start < end
        {
            frame.render_widget(
                Block::default().style(Style::default().bg(bg)),
                Rect::new(
                    area.x,
                    area.y
                        .saturating_add(u16::try_from(start - offset).unwrap_or(0)),
                    area.width,
                    u16::try_from(end - start).unwrap_or(0),
                ),
            );
        }
        row += rows;
    }
}

fn prompt_rows(app: &App, width: u16) -> u16 {
    u16::try_from(
        Paragraph::new(prompt_lines(app, Style::default()))
            .wrap(Wrap { trim: false })
            .line_count(width),
    )
    .unwrap_or(u16::MAX)
}

fn composer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let style = Style::default().fg(Color::White).bg(Color::Rgb(32, 32, 32));
    frame.render_widget(Block::default().style(style), area);
    let input = Rect::new(
        area.x + 3,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    frame.render_widget(
        Paragraph::new("›").style(style),
        Rect::new(area.x + 1, input.y, 1, 1),
    );
    let before = app
        .composer
        .text
        .get(..app.composer.cursor)
        .unwrap_or_default();
    let paragraph = Paragraph::new(prompt_lines(app, style))
        .style(style)
        .wrap(Wrap { trim: false });
    let offset = Paragraph::new(format!("{before}▏"))
        .wrap(Wrap { trim: false })
        .line_count(input.width)
        .saturating_sub(usize::from(input.height));
    frame.render_widget(
        paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        input,
    );
    command_menu(frame, app, area);
}

fn prompt_lines(app: &App, style: Style) -> Vec<Line<'_>> {
    let mut offset = 0;
    let mut lines = Vec::new();
    for line in app.composer.text.split('\n') {
        let end = offset + line.len();
        if (offset..=end).contains(&app.composer.cursor) {
            let cursor = app.composer.cursor - offset;
            let before = line.get(..cursor).unwrap_or_default();
            let tail = line.get(cursor..).unwrap_or_default();
            // A trailing space is discarded by the word wrapper. A visible caret
            // keeps the insertion point present when it wraps past the last column.
            let next = tail.chars().next().unwrap_or('▏');
            let rest = tail.get(next.len_utf8()..).unwrap_or_default();
            let mut spans = vec![
                Span::raw(before),
                Span::styled(
                    next.to_string(),
                    if tail.is_empty() {
                        style
                    } else {
                        style.add_modifier(Modifier::REVERSED)
                    },
                ),
                Span::raw(rest),
            ];
            if app.composer.text.is_empty() {
                spans.push(Span::styled(
                    "Describe a Rust task, or / commands",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(line));
        }
        offset = end + 1;
    }
    lines
}

fn command_menu(frame: &mut Frame<'_>, app: &App, prompt: Rect) {
    if !app.composer.text.starts_with('/') || app.composer.text.contains(char::is_whitespace) {
        return;
    }
    let commands = COMMANDS
        .iter()
        .filter(|(name, _)| name.starts_with(&app.composer.text))
        .map(|(name, help)| {
            Line::from(elide(
                &format!(" {name:10} {help}"),
                usize::from(prompt.width),
            ))
        })
        .collect::<Vec<_>>();
    if commands.is_empty() {
        return;
    }
    let height = u16::try_from(commands.len())
        .unwrap_or(u16::MAX)
        .min(prompt.y.saturating_sub(2));
    let area = Rect::new(
        prompt.x,
        prompt.y.saturating_sub(height),
        prompt.width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(commands).style(Style::default().fg(Color::Gray).bg(Color::Rgb(24, 24, 24))),
        area,
    );
}

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title)
}
fn state_style(label: &str) -> Style {
    let color = if label == "VERIFIED SUCCESS" {
        Color::Green
    } else if matches!(label, "FAILED" | "REJECTED") {
        Color::Red
    } else if matches!(label, "RUNNING" | "QUEUED") {
        Color::Cyan
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
        _ => 28,
    };
    let inset = dialog_area(area, height);
    frame.render_widget(Clear, inset);
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
        Modal::Review { text, scroll, .. } => text_dialog(
            frame,
            "Review host grant",
            text,
            *scroll,
            "y Approve  n Reject  Esc Close",
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
    let height = area.height.saturating_sub(2).min(max_height);
    Rect::new(area.x, area.y + area.height - height, area.width, height)
}

fn modal_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
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
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let rows = paragraph.line_count(body.width);
    let offset = usize::from(scroll).min(rows.saturating_sub(usize::from(body.height)));
    frame.render_widget(
        paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        body,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{hint}\nRows {}–{} of {rows}",
            offset + 1,
            (offset + usize::from(body.height)).min(rows)
        ))
        .wrap(Wrap { trim: false })
        .style(Style::default().add_modifier(Modifier::DIM)),
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
    frame.render_widget(Paragraph::new(objective).wrap(Wrap { trim: false }), intent);
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
            .style(Style::default().fg(Color::DarkGray)),
        explanation,
    );
    frame.render_widget(Paragraph::new("Enter Review grant · Esc Edit"), footer);
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
        Paragraph::new(text).scroll((0, u16::try_from(offset).unwrap_or(u16::MAX))),
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
