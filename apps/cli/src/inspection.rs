use std::io::{self, Write};
use std::path::PathBuf;

#[expect(
    clippy::redundant_pub_crate,
    reason = "Private binary modules use crate-visible entry points to satisfy unreachable_pub."
)]
pub(crate) fn execute(
    mut args: impl Iterator<Item = String>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let run_id = args.next().ok_or("inspect requires a run ID")?;
    let mut store = PathBuf::from(".fishnet/runs");
    while let Some(option) = args.next() {
        match option.as_str() {
            "--store" => store = args.next().ok_or("--store requires a directory")?.into(),
            _ => return Err(format!("unknown inspection option: {option}").into()),
        }
    }
    let record = fishnet_core::inspect(&store, &run_id)?;
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &record)?;
    writeln!(output)?;
    Ok(0)
}
