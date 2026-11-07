use std::error::Error;
use std::io::Write as _;
use std::process::Command;

use fishnet_tui::{app::App, appearance::ColorMode, render};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend, TestBackend},
};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn actual_terminal_bytes_follow_the_selected_colour_policy() -> TestResult {
    for (mode, no_color, colored) in [
        ("auto", false, true),
        ("auto", true, false),
        ("always", true, true),
        ("never", false, false),
    ] {
        // Crossterm caches colour policy globally. Separate child processes also
        // test the real environment contract without changing this test process.
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args(["--ignored", "--exact", "backend_child", "--nocapture"])
            .env("FISHNET_BACKEND_TEST_MODE", mode)
            .env_remove("NO_COLOR");
        if no_color {
            child.env("NO_COLOR", "1");
        }
        let output = child.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout)?;
        assert!(text.contains("Describe a Rust task"));
        assert!(text.contains("host default"));
        assert_eq!(
            text.contains("38;2;135;163;218"),
            colored,
            "{mode}, NO_COLOR={no_color}"
        );
        assert!(
            !text.contains("\u{1b}[;m"),
            "an empty colour sequence loses the panel styles"
        );
        if !colored {
            assert!(
                text.contains("\u{1b}[7m"),
                "monochrome panels need reverse video"
            );
            assert!(text.contains("monochrome"));
        }
    }
    Ok(())
}

#[test]
#[ignore = "Invoked by the colour-policy test in a process with an isolated environment."]
fn backend_child() -> TestResult {
    let mode =
        ColorMode::parse(&std::env::var("FISHNET_BACKEND_TEST_MODE")?).ok_or("invalid mode")?;
    let mut app = App::default();
    app.appearance = mode.apply();
    app.connected = true;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|frame| render::draw(frame, &mut app))?;
    let mut bytes = Vec::new();
    {
        let mut backend = CrosstermBackend::new(&mut bytes);
        backend.draw(terminal.backend().buffer().content.iter().enumerate().map(
            |(index, cell)| {
                (
                    u16::try_from(index % 80).unwrap_or(0),
                    u16::try_from(index / 80).unwrap_or(0),
                    cell,
                )
            },
        ))?;
        Backend::flush(&mut backend)?;
    }
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}
