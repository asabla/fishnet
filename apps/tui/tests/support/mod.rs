use std::error::Error;
use std::fmt::Write as _;

use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
    text::Line,
};

/// Export the actual `TestBackend` cells for manual visual review. This is test evidence,
/// not an application web client. The default suite writes no preview files.
///
/// # Errors
/// Returns a formatting or filesystem error when an export is requested.
pub fn export(name: &str, buffer: &Buffer) -> Result<(), Box<dyn Error>> {
    let Some(directory) = std::env::var_os("FISHNET_LAYOUT_PREVIEW_DIR") else {
        return Ok(());
    };
    let area = buffer.area;
    let mut html = format!(
        "<!doctype html><meta charset=utf-8><title>fishnet {name} {}x{}</title><style>body{{margin:24px;background:#181818}}pre{{display:inline-block;margin:0;padding:12px;background:#101010;color:#d7d7d7;font:14px/20px Menlo,monospace}}span{{display:inline-block;vertical-align:top}}</style><pre>",
        area.width, area.height
    );
    for row in buffer.content.chunks(usize::from(area.width).max(1)) {
        let mut skip = 0;
        for cell in row {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let symbol = cell.symbol();
            let width = Line::from(symbol).width().max(1);
            skip = width.saturating_sub(1);
            let mut fg = color(cell.fg, "#d7d7d7");
            let mut bg = color(cell.bg, "#101010");
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let weight = if cell.modifier.contains(Modifier::BOLD) {
                "bold"
            } else {
                "normal"
            };
            let symbol = symbol
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            write!(
                html,
                "<span style=\"width:{width}ch;color:{fg};background:{bg};font-weight:{weight}\">{symbol}</span>"
            )?;
        }
        html.push('\n');
    }
    html.push_str("</pre>");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        std::path::Path::new(&directory).join(format!("{name}.html")),
        html,
    )?;
    Ok(())
}

fn color(color: Color, default: &str) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#000000".into(),
        Color::White => "#ffffff".into(),
        Color::Gray => "#c0c0c0".into(),
        Color::DarkGray => "#808080".into(),
        Color::Red => "#cd3131".into(),
        Color::Green => "#0dbc79".into(),
        Color::Cyan => "#11a8cd".into(),
        Color::Yellow => "#e5e510".into(),
        _ => default.into(),
    }
}
