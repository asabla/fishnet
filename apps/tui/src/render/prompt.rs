use super::{elide, theme};
use crate::app::{App, COMMANDS};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

// Top and bottom padding plus the model row surround the editable text.
const FRAME_ROWS: u16 = 3;

pub(super) fn height(app: &App, width: u16, available: u16) -> u16 {
    // Preserve two visible input rows on small screens. Larger prompts remain bounded.
    rows(app, width.saturating_sub(4))
        .clamp(1, (available / 3).saturating_sub(FRAME_ROWS).max(2))
        .saturating_add(FRAME_ROWS)
}

fn rows(app: &App, width: u16) -> u16 {
    u16::try_from(
        Paragraph::new(prompt_lines(app, Style::default()))
            .wrap(Wrap { trim: false })
            .line_count(width),
    )
    .unwrap_or(u16::MAX)
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let style = theme::surface();
    frame.render_widget(Block::default().style(style), area);
    let input = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(3),
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme::ACCENT)),
        area,
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
    metadata(
        frame,
        app,
        Rect::new(input.x, area.bottom().saturating_sub(2), input.width, 1),
    );
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
                    "Describe a Rust task…",
                    theme::muted().bg(theme::SURFACE),
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

pub(super) fn command_menu(frame: &mut Frame<'_>, app: &App, prompt: Rect) {
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
    frame.render_widget(Paragraph::new(commands).style(theme::surface()), area);
}

fn metadata(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let model = if app.default_model.is_empty() {
        "host default"
    } else {
        &app.default_model
    };
    let label = elide(
        &format!(" · {model}"),
        usize::from(area.width).saturating_sub(4),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Rust", theme::surface().fg(theme::ACCENT)),
            Span::styled(label, theme::surface()),
        ])),
        area,
    );
}
