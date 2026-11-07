use ratatui::style::{Color, Style};

pub(super) const BACKGROUND: Color = Color::Rgb(12, 12, 12);
pub(super) const SURFACE: Color = Color::Rgb(28, 28, 28);
pub(super) const TEXT: Color = Color::Rgb(224, 224, 224);
pub(super) const MUTED: Color = Color::Rgb(137, 137, 137);
pub(super) const ACCENT: Color = Color::Rgb(135, 163, 218);

pub(super) const fn base() -> Style {
    Style::new().fg(TEXT).bg(BACKGROUND)
}

pub(super) const fn muted() -> Style {
    base().fg(MUTED)
}

pub(super) const fn surface() -> Style {
    base().bg(SURFACE)
}
