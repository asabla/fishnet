use std::io::Write;
use std::process::ExitCode;
fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let result = if first.as_deref() == Some("serve") {
        fishnet_service::daemon::launch(args)
    } else {
        fishnet_tui::terminal::launch(first.into_iter().chain(args))
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "fishnet-tui: {error}");
            ExitCode::FAILURE
        }
    }
}
