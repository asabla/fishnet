use std::io::Write;
use std::process::ExitCode;
fn main() -> ExitCode {
    match fishnet_service::daemon::launch(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "fishnetd: {error}");
            ExitCode::FAILURE
        }
    }
}
