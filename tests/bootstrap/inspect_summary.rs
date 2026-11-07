// Independent acceptance case for the first change made through fishnet.
// Compile with rustc --test before starting the agent. Keep this source protected.
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    option_env!("CARGO_BIN_EXE_fishnet")
        .map_or_else(|| PathBuf::from("target/debug/fishnet"), PathBuf::from)
}

#[test]
fn compact_inspection_retains_outcome_and_counts_without_full_events() -> Result<(), Box<dyn Error>>
{
    let store = std::env::temp_dir().join(format!("fishnet-summary-{}", std::process::id()));
    fs::create_dir_all(&store)?;
    let result = Command::new(binary())
        .args(["run", "--scenario", "success", "--store"])
        .arg(&store)
        .output()?;
    assert!(result.status.success());
    let entry = fs::read_dir(&store)?.next().ok_or("run record missing")??;
    let run_id = entry
        .path()
        .file_stem()
        .ok_or("run ID missing")?
        .to_string_lossy()
        .into_owned();
    let bytes_before = fs::read(entry.path())?;
    let compact = Command::new(binary())
        .args(["inspect", &run_id, "--store"])
        .arg(&store)
        .arg("--summary")
        .output()?;
    assert!(
        compact.status.success(),
        "summary command failed: {}",
        String::from_utf8_lossy(&compact.stderr)
    );
    let text = String::from_utf8(compact.stdout)?;
    assert!(text.contains(&run_id));
    assert!(text.contains("\"state\":\"verified_success\""));
    assert!(text.contains("\"incomplete\":false"));
    assert!(text.contains("\"logical_tool_calls\":1"));
    assert!(text.contains("\"provider_attempts\":2"));
    assert!(!text.contains("\"events\""));
    assert!(!text.contains("operation_started"));
    assert!(text.len() < 4_096);
    let full = Command::new(binary())
        .args(["inspect", &run_id, "--store"])
        .arg(&store)
        .output()?;
    assert!(full.status.success());
    assert!(String::from_utf8(full.stdout)?.contains("\"events\""));
    assert_eq!(fs::read(entry.path())?, bytes_before);
    fs::remove_dir_all(store)?;
    Ok(())
}
