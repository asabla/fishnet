use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, Modal, safe, status};

const HELP: &str = "Start work\n\nn       Describe a Rust task and name the files to change\nf       Run an offline fixture; no account is needed\np       Open an advanced task file\n\nTasks and approvals\n\nTab     Show or hide the task list\nUp/Down Select a task\na/Enter Review the selected pending grant\nc       Open cancellation confirmation\n\nReading\n\nPageUp/PageDown  Scroll wrapped output\nEnd             Follow the latest output\nv               Full task identity, diagnostics, and recent status\nr               Refresh the connection\n\nForms\n\nTab/Shift-Tab    Change field\nLeft/Right      Move the text cursor\nHome/End        Start or end of field\nCtrl-U          Clear the field\nEnter           Continue to grant review\nEsc             Close without submitting\n\nApproval review\n\ny approves; n rejects; arrows scroll; Esc closes.\nThe full provider, file grant, commands, checks, and limits are shown.\nOnly host checks can establish verified success.\n\nLeaving\n\nq or Ctrl-C detaches. Accepted work continues in the service.\nUse fishnet stop outside the TUI to stop the service.\nCancellation can leave completed edits. It does not roll them back.";

/// Render task content first. Detailed diagnostics remain in the details view.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    if area.width < 40 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("fishnet\nUse at least 40 columns and 12 rows.\nq or Ctrl-C detaches.")
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let [heading, body, notice, keys] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .areas(area);
    normalize_scroll(app, body, area);
    header(frame, app, heading);
    if app.show_tasks && area.width >= 110 {
        let [tasks, content] = Layout::horizontal([Constraint::Length(28), Constraint::Min(40)])
            .spacing(2)
            .areas(body);
        task_list(frame, app, tasks);
        output(frame, app, content);
    } else if app.show_tasks {
        task_list(frame, app, body);
    } else {
        output(frame, app, body);
    }
    frame.render_widget(
        Paragraph::new(safe(&app.notice, 1024))
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: false }),
        notice,
    );
    frame.render_widget(
        Paragraph::new(if area.width < 60 {
            "n New  Tab Tasks  a Review  c Cancel\nv Details  ? Help  q Detach"
        } else {
            "n New task   Tab Tasks   a Review   c Cancel\nv Details    ? Help      q Detach"
        })
        .style(Style::default().add_modifier(Modifier::DIM)),
        keys,
    );
    if let Some(modal) = &app.modal {
        dialog(frame, app, modal, area);
    }
}

fn normalize_scroll(app: &mut App, body: Rect, area: Rect) {
    let width = if app.show_tasks && area.width >= 110 {
        body.width.saturating_sub(30)
    } else {
        body.width
    };
    let lines = app
        .lines
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect::<Vec<_>>();
    let rows = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .line_count(width);
    let max = rows.saturating_sub(usize::from(body.height.saturating_sub(1)));
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
    let rows = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .line_count(dialog.width.saturating_sub(2));
    let max = rows.saturating_sub(usize::from(dialog.height.saturating_sub(4)));
    *scroll = (*scroll).min(u16::try_from(max).unwrap_or(u16::MAX));
}

fn header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let connection = if app.connected {
        "Connected"
    } else {
        "Disconnected"
    };
    let title = Line::from(vec![
        Span::styled("fishnet", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  ·  {connection}")),
    ]);
    frame.render_widget(Paragraph::new(vec![title, caption(app, area.width)]), area);
}
fn caption(app: &App, width: u16) -> Line<'static> {
    let Some(view) = &app.snapshot else {
        return Line::from("n New task  ·  f Offline fixture");
    };
    let state = status(
        view.phase,
        view.run.as_ref().map(|run| run.state),
        view.run.as_ref().is_some_and(|run| run.incomplete),
    );
    let objective = app
        .tasks
        .iter()
        .find(|task| task.task_id == view.task_id)
        .map_or_else(
            || {
                view.approval
                    .as_ref()
                    .map_or("", |grant| grant.objective.as_str())
            },
            |task| task.objective.as_str(),
        );
    let available = usize::from(width).saturating_sub(state.len() + 3);
    let objective = elide(objective, available);
    let mut spans = vec![Span::styled(state, state_style(state))];
    if !objective.is_empty() {
        spans.push(Span::raw(format!(" · {objective}")));
    }
    Line::from(spans)
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
            Paragraph::new(
                "No tasks yet.\n\nn  New Rust task\nf  Offline fixture\np  Advanced task file",
            )
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
            .block(panel("Tasks · up/down"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> "),
        area,
        &mut state,
    );
}

fn output(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.snapshot.is_none() && app.lines.is_empty() {
        let welcome = "\nWhat do you want to work on?\n\nn  New task\n   Describe the change and name exact files.\n   Review the provider, checks, and file access before execution.\n\nf  Try an offline fixture\n   Exercise the full model/tool/check cycle without an account.\n\nChatGPT setup: fishnet auth login\nAdvanced task files: p\n\nAccepted tasks continue when this interface detaches.";
        frame.render_widget(
            Paragraph::new(welcome)
                .wrap(Wrap { trim: false })
                .block(panel("Workspace")),
            area,
        );
        return;
    }
    let lines = if app.lines.is_empty() {
        let default = if app
            .snapshot
            .as_ref()
            .is_some_and(|view| view.approval.is_some())
        {
            "Waiting for grant review. Press a or Enter."
        } else {
            "Waiting for task output. Details: v."
        };
        let message = app
            .snapshot
            .as_ref()
            .and_then(|view| view.message.as_deref())
            .map_or_else(|| default.to_owned(), |message| safe(message, 2048));
        vec![Line::from(message)]
    } else {
        app.lines
            .iter()
            .map(|line| Line::styled(line.clone(), line_style(line)))
            .collect()
    };
    let block = panel(if app.scroll == 0 {
        "Activity · following"
    } else {
        "Activity · history · End to follow"
    });
    let inner = block.inner(area);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let last = paragraph
        .line_count(inner.width)
        .saturating_sub(usize::from(inner.height));
    let offset = last.saturating_sub(usize::from(app.scroll));
    frame.render_widget(
        paragraph
            .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
            .block(block),
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
fn line_style(line: &str) -> Style {
    if line.starts_with("Outcome:") || line.starts_with("Independent check:") {
        Style::default().add_modifier(Modifier::BOLD)
    } else if line.starts_with("Operation error:") || line.starts_with("Rejected:") {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    }
}
fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

fn dialog(frame: &mut Frame<'_>, app: &App, modal: &Modal, area: Rect) {
    let height = match modal {
        Modal::Draft { .. } => 16,
        Modal::Code { .. } => 12,
        Modal::Cancel(_) => 10,
        _ => 28,
    };
    let inset = dialog_area(area, height);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new("fishnet").style(Style::default().add_modifier(Modifier::BOLD)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    match modal {
        Modal::Draft {
            fields,
            focus,
            cursor,
        } => draft_form(frame, app, fields, *focus, *cursor, inset),
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
    let width = area.width.saturating_sub(4).min(90);
    let height = area.height.saturating_sub(2).min(max_height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn modal_block(title: &str) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
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

fn draft_form(
    frame: &mut Frame<'_>,
    app: &App,
    fields: &[String; 2],
    focus: usize,
    cursor: usize,
    area: Rect,
) {
    let block = modal_block("New Rust task");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intro, objective, files, footer] = Layout::vertical([
        Constraint::Length(if inner.height >= 12 { 2 } else { 1 }),
        Constraint::Min(2),
        Constraint::Length(2),
        Constraint::Length(if inner.height >= 12 { 2 } else { 1 }),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new("Describe the result; name exact files.").wrap(Wrap { trim: false }),
        intro,
    );
    for (index, ((label, value), field_area)) in ["Task", "Files to change (comma-separated)"]
        .into_iter()
        .zip(fields)
        .zip([objective, files])
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
        Paragraph::new(format!(
            "Tab Next  Enter Review  Esc Close\nRust fmt/test/Clippy/docs · Account: {}",
            safe(&app.account, 64)
        ))
        .style(Style::default().add_modifier(Modifier::DIM)),
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
