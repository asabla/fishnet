use std::io::Write;
use std::process::ExitCode;
fn main() -> ExitCode {
    match fishnet_tui::terminal::launch(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "fishnet-tui: {error}");
            ExitCode::FAILURE
        }
    }
}
