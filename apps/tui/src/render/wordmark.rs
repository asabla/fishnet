use super::theme;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

const WORDMARK: [&str; 3] = [
    "█▀▀▀▀ ▀█▀ ▄▀▀▀▀ █   █ █▄  █ █▀▀▀▀ ▀▀█▀▀",
    "█▀▀▀   █   ▀▀▀▄ █▀▀▀█ █ ▀▄█ █▀▀▀    █  ",
    "▀     ▀▀▀ ▀▀▀▀  ▀   ▀ ▀   ▀ ▀▀▀▀▀   ▀  ",
];

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect) {
    if area.height < 3 {
        frame.render_widget(
            Paragraph::new("fishnet")
                .alignment(Alignment::Center)
                .style(theme::base().add_modifier(Modifier::BOLD)),
            area,
        );
        return;
    }
    let lines = WORDMARK
        .into_iter()
        .map(|row| {
            Line::from(vec![
                Span::styled(row.chars().take(22).collect::<String>(), theme::muted()),
                Span::styled(row.chars().skip(22).collect::<String>(), theme::base()),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}
