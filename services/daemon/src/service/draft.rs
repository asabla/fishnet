use std::path::PathBuf;

use fishnet_core::{
    Limits,
    process::CommandSpec,
    workspace::{TaskFile, Workspace, WorkspaceSpec},
};

use crate::contract::{ApiError, Result};

/// Construct a fixed host profile. Clients supply intent and exact files, never argv.
pub(super) fn prepare(root: PathBuf, objective: String, files: Vec<String>) -> Result<TaskFile> {
    if objective.trim().is_empty() || objective.len() > 4096 {
        return Err(ApiError::invalid("describe the task in 1..=4096 bytes"));
    }
    if !root.join("Cargo.toml").is_file() {
        return Err(ApiError::invalid(
            "the guided Rust task requires Cargo.toml; use a reviewed task file for other projects",
        ));
    }
    let cargo = std::env::var_os("PATH")
        .and_then(|value| {
            std::env::split_paths(&value)
                .map(|directory| directory.join("cargo"))
                .find(|path| path.is_absolute() && path.is_file())
        })
        .ok_or_else(|| ApiError::invalid("cargo is unavailable in the service PATH"))?;
    let commands = [
        ("format", vec!["fmt", "--all", "--", "--check"]),
        ("test", vec!["test", "--workspace", "--offline", "--locked"]),
        (
            "lint",
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--offline",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        (
            "docs",
            vec!["doc", "--workspace", "--no-deps", "--offline", "--locked"],
        ),
    ]
    .into_iter()
    .map(|(id, args)| CommandSpec {
        id: id.into(),
        program: cargo.clone(),
        arguments: args.into_iter().map(String::from).collect(),
        timeout_ms: 120_000,
    })
    .collect::<Vec<_>>();
    let protected_paths = ["AGENTS.md", "rust-toolchain.toml", "scripts/check.sh"]
        .into_iter()
        .filter(|path| root.join(path).is_file())
        .map(String::from)
        .collect();
    let workspace = Workspace::open(WorkspaceSpec {
        root,
        required_changes: files.clone(),
        allowed_writes: files,
        protected_paths,
        checks: commands.iter().map(|command| command.id.clone()).collect(),
        commands,
    })
    .map_err(|error| ApiError::invalid(&error.message))?;
    Ok(TaskFile {
        schema_version: 1,
        objective,
        workspace: workspace.spec().clone(),
        limits: Limits {
            deadline_ms: 300_000,
            max_provider_calls: 32,
            max_output_bytes: 8192,
        },
    })
}
