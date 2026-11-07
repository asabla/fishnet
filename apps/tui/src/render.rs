use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, Modal, safe, status};

/// Render a bounded service view. Color supplements explicit outcome labels.
pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 60 || area.height < 15 {
        frame.render_widget(Paragraph::new("fishnet\nTerminal too small. Use at least 60 columns and 15 rows.\nq or Ctrl-C detaches without cancelling work."),area);
        return;
    }
    let [heading, body, notice, keys] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .areas(area);
    header(frame, app, heading);
    let [tasks, events] =
        Layout::horizontal([Constraint::Percentage(34), Constraint::Percentage(66)]).areas(body);
    task_list(frame, app, tasks);
    output(frame, app, events);
    frame.render_widget(
        Paragraph::new(safe(&app.notice, 1024))
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Status")),
        notice,
    );
    frame.render_widget(
        Paragraph::new(
            " n fixture  p code  a approval  c cancel  r refresh  v status  ? help  q/Ctrl-C detach",
        )
        .wrap(Wrap { trim: false }),
        keys,
    );
    if let Some(modal) = &app.modal {
        dialog(frame, modal, area);
    }
}

fn header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let connected = if app.connected {
        "CONNECTED"
    } else {
        "DISCONNECTED - last known state"
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("fishnet   {connected}"),
        Style::default()
            .fg(if app.connected {
                Color::Cyan
            } else {
                Color::Yellow
            })
            .add_modifier(Modifier::BOLD),
    ))];
    if let Some(view) = &app.snapshot {
        let state = view.run.as_ref().map(|r| r.state);
        let incomplete = view.run.as_ref().is_some_and(|r| r.incomplete);
        lines.push(Line::from(format!(
            "{}   task {}",
            status(view.phase, state, incomplete),
            view.task_id
        )));
        if let Some(run) = &view.run {
            lines.push(Line::from(format!(
                "Model {}  Tools {}  Processes {}  Rejected {}  Pending {}",
                run.statistics.provider_attempts,
                run.statistics.logical_tool_calls,
                run.statistics.process_attempts,
                run.statistics.rejected_calls,
                run.pending_operation_ids.len()
            )));
        } else {
            lines.push(Line::from(
                "Task state comes from the service; model text cannot verify success.",
            ));
        }
    } else {
        lines.push(Line::from(
            "Select a task, or press n to run an offline fixture.",
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL)),
        area,
    );
}
fn task_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = app
        .tasks
        .iter()
        .map(|task| {
            let label = status(task.phase, task.state, false);
            ListItem::new(vec![
                Line::from(Span::styled(
                    format!(
                        "{}  {label}",
                        task.task_id.get(..8).unwrap_or(&task.task_id)
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(safe(&task.objective, 160)),
            ])
        })
        .collect::<Vec<_>>();
    let selected = app
        .tasks
        .iter()
        .position(|task| Some(&task.task_id) == app.selected.as_ref());
    let mut state = ListState::default().with_selected(selected);
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Tasks ({}) - up/down", app.tasks.len())),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}
fn output(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let visible = usize::from(area.height.saturating_sub(2));
    let end = app.lines.len().saturating_sub(usize::from(app.scroll));
    let start = end.saturating_sub(visible);
    let lines = if app.lines.is_empty() {
        vec![
            Line::from("Durable event output appears here."),
            Line::from("PageUp/PageDown scroll; End follows latest output."),
        ]
    } else {
        app.lines
            .iter()
            .skip(start)
            .take(visible)
            .map(|line| Line::from(line.clone()))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((0, app.horizontal)).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Recent output - full journal retained"),
        ),
        area,
    );
}
fn dialog(frame: &mut Frame<'_>, modal: &Modal, area: Rect) {
    let inset = Rect {
        x: area.x.saturating_add(3),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(6),
        height: area.height.saturating_sub(2),
    };
    frame.render_widget(Clear, inset);
    match modal {
        Modal::Code {fields,focus}=>code_form(frame,fields,*focus,inset),
        Modal::Review {text,scroll,..}=>frame.render_widget(Paragraph::new(text.as_str()).wrap(Wrap {trim:false}).scroll((*scroll,0)).block(Block::default().borders(Borders::ALL).title("Review host grant: y / n / Esc")),inset),
        Modal::Cancel(id)=>frame.render_widget(Paragraph::new(format!("Cancel task {id}?\nCompleted edits can remain.\n\ny confirms cancellation. Any other key closes this dialog.")).wrap(Wrap {trim:false}).block(Block::default().borders(Borders::ALL).title("Cancel task")),inset),
        Modal::Notice {text,scroll}=>frame.render_widget(Paragraph::new(text.as_str()).wrap(Wrap {trim:false}).scroll((*scroll,0)).block(Block::default().borders(Borders::ALL).title("Recent status - arrows scroll / Esc close")),inset),
        Modal::Help=>frame.render_widget(Paragraph::new("n     Start an offline success fixture\np     Submit a reviewed coding task from tasks/\na     Review the selected pending grant\nc     Confirm cancellation of the selected task\nq     Detach; accepted work continues\nCtrl-C detaches, including from a dialog\nUp/Down or j/k selects a task\nPageUp/PageDown scrolls recent output\nLeft/Right scrolls output horizontally\nEnd   Follow latest output\nv     Recent status and uncertain command identities\nr     Refresh connection and task list\n\nApprovals: y approves, n rejects, arrows scroll, Esc closes.\nCoding form: Tab changes fields, Enter submits, Esc closes.\nOutput contains completed durable events, not transient tokens.\nVerified success requires independent host checks.\n\nAny key closes help.").wrap(Wrap {trim:false}).block(Block::default().borders(Borders::ALL).title("Help")),inset),
    }
}
fn code_form(frame: &mut Frame<'_>, fields: &[String; 3], focus: usize, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Coding task - Tab fields / Enter submit / Esc close");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intro, path, model, account] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new("Use a reviewed host task under tasks/. Review its grant before execution.")
            .wrap(Wrap { trim: false }),
        intro,
    );
    for (index, ((label, value), field_area)) in [
        "Task file",
        "Model (blank = host/account default)",
        "ChatGPT account",
    ]
    .into_iter()
    .zip(fields)
    .zip([path, model, account])
    .enumerate()
    {
        let style = if focus == index {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let text = safe(value, 512);
        let width = Line::from(text.as_str()).width();
        let visible = usize::from(field_area.width.saturating_sub(3));
        let offset = if focus == index {
            width.saturating_sub(visible)
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(text)
                .scroll((0, u16::try_from(offset).unwrap_or(u16::MAX)))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(label)
                        .border_style(style),
                ),
            field_area,
        );
        if focus == index {
            frame.set_cursor_position((
                field_area
                    .x
                    .saturating_add(1)
                    .saturating_add(u16::try_from(width.min(visible)).unwrap_or(0)),
                field_area.y.saturating_add(1),
            ));
        }
    }
}
