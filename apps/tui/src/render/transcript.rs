use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::theme;
use crate::app::{App, Entry, EntryKind};

struct Message<'a> {
    paragraph: Paragraph<'a>,
    height: usize,
    edge: Option<Color>,
}

impl<'a> Message<'a> {
    fn new(entry: &'a Entry, width: u16) -> Self {
        let edge = match entry.kind {
            EntryKind::User => Some(theme::ACCENT),
            EntryKind::Tool => Some(Color::Rgb(60, 60, 60)),
            EntryKind::Error => Some(Color::Red),
            _ => None,
        };
        let style = if edge.is_some() {
            theme::surface()
        } else {
            theme::base()
        };
        let lines = entry
            .text
            .lines()
            .enumerate()
            .map(|(index, text)| {
                let style = match entry.kind {
                    EntryKind::Tool if index > 0 => style.fg(theme::MUTED),
                    EntryKind::Tool | EntryKind::Outcome if index == 0 => {
                        style.add_modifier(Modifier::BOLD)
                    }
                    EntryKind::Check => style.fg(theme::ACCENT),
                    EntryKind::Error => style.fg(Color::Red),
                    _ => style,
                };
                let text = if index == 0 && entry.kind == EntryKind::Tool {
                    tool_title(text)
                } else {
                    text.into()
                };
                Line::styled(text, style)
            })
            .collect::<Vec<_>>();
        let paragraph = Paragraph::new(lines)
            .style(style)
            .wrap(Wrap { trim: false });
        let height = paragraph.line_count(width.saturating_sub(3));
        Self {
            paragraph,
            height,
            edge,
        }
    }
}

pub(super) fn rows(app: &App, width: u16) -> usize {
    app.transcript
        .iter()
        .map(|entry| Message::new(entry, width).height + 1)
        .sum()
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.transcript.is_empty() {
        let text = app
            .snapshot
            .as_ref()
            .and_then(|view| view.message.as_deref())
            .unwrap_or("Waiting for task events. /review opens pending access.");
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .style(theme::muted()),
            area,
        );
        return;
    }
    let messages = app
        .transcript
        .iter()
        .map(|entry| Message::new(entry, area.width))
        .collect::<Vec<_>>();
    let total: usize = messages.iter().map(|message| message.height + 1).sum();
    let offset = total
        .saturating_sub(usize::from(area.height))
        .saturating_sub(usize::from(app.scroll));
    let mut row = 0;
    for message in messages {
        let start = row.max(offset);
        let end = (row + message.height).min(offset + usize::from(area.height));
        if start < end {
            let visible = Rect::new(
                area.x,
                area.y + u16::try_from(start - offset).unwrap_or(0),
                area.width,
                u16::try_from(end - start).unwrap_or(0),
            );
            if let Some(edge) = message.edge {
                frame.render_widget(
                    Block::default()
                        .style(theme::surface())
                        .borders(Borders::LEFT)
                        .border_style(theme::surface().fg(edge)),
                    visible,
                );
            }
            let text = Rect::new(
                visible.x + 2,
                visible.y,
                visible.width.saturating_sub(3),
                visible.height,
            );
            frame.render_widget(
                message
                    .paragraph
                    .scroll((u16::try_from(start - row).unwrap_or(0), 0)),
                text,
            );
        }
        row += message.height + 1;
    }
}

fn tool_title(text: &str) -> String {
    for (name, label) in [
        ("read_file ", "Read"),
        ("replace_text ", "Edit"),
        ("run_command ", "Run"),
        ("list_files ", "List"),
    ] {
        if let Some(target) = text.strip_prefix(name) {
            return format!("{label}  {target}");
        }
    }
    text.into()
}
