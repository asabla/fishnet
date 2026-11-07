# ADR-0007: Interactive entry and guided Rust tasks

Status: accepted, 2026-11-07.

## Context

M4's initial acceptance checks covered the service contract and terminal
mechanics. They did not establish a usable product entry point. Normal work
required two terminals, task/account output defaulted to JSON, and users had to
author JSON before a coding task. Fixed pane sizes, unwrapped output, and
unscrollable help made the display difficult to use. The user rejected this
state as too crude.

## Decision

Make the installed `fishnet` command the normal entry. In an interactive
terminal, no arguments opens the TUI. Without an interactive terminal, it prints
help and has no startup effects. `fishnet tui` remains explicit. The CLI links
the existing TUI library rather than finding an unrelated executable on PATH.
The four workspace packages remain the same.

For the default connection only, the client checks the existing service. If
needed it starts the same executable in service mode, with coding preparation
enabled. The child has null stdin, private bounded startup logs, and a separate
Unix process group. It owns tasks after client detachment. No provider access
occurs at startup. Explicit remote/custom connections only attach. A service
store still has one exclusive owner. An authenticated shutdown command stops
admission, cancels active work, and waits for cleanup.

The launcher uses the standard library's
[process-group control](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.process_group).
It does not invoke a shell or another agent engine to start the service.

Add a typed Rust task request containing objective, exact file paths, account,
and optional model. The host prepares the immutable grant and resolves Cargo
from its own PATH. It selects fixed fmt, test, Clippy, and documentation argv;
test, lint, and documentation dependency resolution is offline and locked.
Clients cannot supply shell text or executable argv through this request. Every
coding request still requires explicit approval. Advanced reviewed task files
remain available for other projects, checks, or file grants.

The guided profile requires all named source files to change and preserves
existing root instructions, toolchain, and quality script files. Its quality
checks do not establish every behavioral property of an objective. Review the
diff and use protected behavior checks where needed. Commands retain host OS
permissions; this profile does not provide an OS sandbox.

Task and account commands show readable output by default. `--json` preserves
machine-readable output. Unique task prefixes reduce copying of opaque IDs.
`make run` opens the app; `make fixture` retains the offline fixture utility.
These are deliberate interface changes; scripts must select JSON explicitly.

The terminal view uses wrapped activity, a separate details view, adaptive task
selection, and focused modal backgrounds. Use Ratatui's pinned
`unstable-rendered-line-info` feature to calculate wrapped viewport bounds with
the same implementation that renders them. This is a library API feature, not a
nightly Rust requirement. It avoids a second Unicode wrapping implementation.
Review its behavior when upgrading Ratatui; row snapshots and scroll tests cover
resize and end-of-output behavior.

Normal q waits for queued command acknowledgements. Ctrl-C detaches immediately
and prints retained pending or uncertain identities after restoring the
terminal. A lost response does not cause an automatic command retry. Known
provider initialization failures before the core loop are failed tasks;
uncertain interrupted execution remains unknown.

## Evidence and limits

See [M4 evidence](../m4-evidence.md). Default tests remain offline and require no
account or interactive terminal. Actual local process tests are separate from
headless layout snapshots. Native Terminal access was blocked in the current
computer-use session; those checks do not establish visual compatibility with
that app or every terminal emulator.

The automatic host uses the working tree's account and default ChatGPT profile.
Other provider profiles remain explicit host options. This change adds no
plugin runtime, memory generation, automatic policy promotion, or M5 feature.
Upgrade service and client together; older builds do not support the new guided
request or shutdown command.
