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
    let mut summary = false;
    while let Some(option) = args.next() {
        match option.as_str() {
            "--store" => store = args.next().ok_or("--store requires a directory")?.into(),
            "--summary" => summary = true,
            _ => return Err(format!("unknown inspection option: {option}").into()),
        }
    }

    let record = fishnet_core::inspect(&store, &run_id)?;
    let mut output = io::stdout().lock();
    if summary {
        let summary = serde_json::json!({
            "run_id": record.run_id,
            "state": record.state,
            "outcome": record.outcome,
            "incomplete": record.incomplete,
            "partial_tail": record.partial_tail,
            "pending_operation_ids": record.pending_operation_ids,
            "statistics": record.statistics,
        });
        serde_json::to_writer(&mut output, &summary)?;
    } else {
        serde_json::to_writer(&mut output, &record)?;
    }
    writeln!(output)?;
    Ok(0)
}
