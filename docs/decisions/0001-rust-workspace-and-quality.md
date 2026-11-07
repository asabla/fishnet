# ADR-0001: Small Rust workspace and strict built-in checks

Status: accepted

Date: 2026-11-07

## Problem and constraints

M1 must own a local model/tool loop. Tests must not need a provider, credentials,
or network access. The user requires strict formatting and code quality checks.

## Options considered

A single binary would mix runtime behavior with CLI concerns. A crate for every
proposed architecture component would add boundaries without working behavior.
Tokio and Clap are candidates, but M1 has no concurrent I/O, streaming API, or
complex command grammar that needs them.

## Decision

Use Rust 1.94.1, edition 2024, pinned with rustfmt and Clippy. Use one core library
and one thin CLI crate. Keep protocol types in the core. The synchronous core
owns mutable state. A condition variable and shared cancellation flag interrupt
fixture waits. No async workers or queues are needed for M1.

Use exact Serde 1.0.228 and serde_json 1.0.150 versions for versioned JSON records.
Use exact ctrlc 3.5.2 in the CLI for signal delivery. Retain the complete Cargo
lockfile. These versions were available locally and built offline on the host.
The core has no process execution dependency or external agent runtime.

Use the workspace lint settings and the common check script described in
[development policy](../development.md). Pin the compiler so new Clippy releases
do not silently change the accepted lint set. A toolchain update must rerun the
same checks. Forbid unsafe code. Keep overflow checks in release builds.

## Evidence and validation

Read the [Serde documentation](https://docs.rs/serde/1.0.228/serde/),
[ctrlc documentation](https://docs.rs/ctrlc/3.5.2/ctrlc/),
[Cargo lint contract](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section),
and [Clippy 1.94 lint reference](https://clippy.rust-lang.org/rust-1.94.0/index.html).
Inspect the cached serde_json 1.0.150 source and manifest. Validate the selected
rules with the pinned local compiler. See [M1 evidence](../m1-evidence.md) for
actual checks. No performance claim follows from this dependency choice.

## Consequences and compatibility

The CLI grammar and fixtures are deliberately small. Blocking storage and output
can delay cooperative deadline handling. The current host is macOS on ARM64.
Linux CI is defined; its result must be reported separately.

## Reversal or migration path

Keep the command/event types stable when adding an async provider or service.
Split another crate only when a tested boundary needs it. Add an async runtime
when M2 I/O requires it. No middleware runtime or provider is selected here.

## Developer command entry point, 2026-11-07

The user requested Make commands and discoverable help. Use a small Makefile
with a help default and explicit `.PHONY` command targets. Keep
`./scripts/check.sh` as the common quality command for local development and CI.
Make wraps existing Cargo commands; it does not become a runtime dependency.
Direct Cargo commands remain available. Verify the recipes with the host's GNU
Make 3.81. No additional task runner is needed.
