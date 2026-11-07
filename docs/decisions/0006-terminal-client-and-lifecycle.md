# ADR-0006: Terminal client and owned client lifecycle

Status: accepted

Date: 2026-11-07

## Problem and constraints

M4 needs a working terminal client of M3. Input, layout, cancellation, approvals,
and reconnect must be testable without a model, account, network, or terminal.
The TUI must not become another execution engine or authorization authority.

## Decision

Add the fourth workspace package, `fishnet-tui`, under `apps/tui`. Use pinned
Ratatui 0.30.2 with only its Crossterm 0.29 backend feature. Pin Crossterm 0.29.0.
Use the existing service client and typed contract. Keep input/state, rendering,
network I/O, and terminal lifetime in separate modules.

Read and poll terminal input on the same main thread. A separate owned thread
runs asynchronous client I/O. Both command and observation channels have capacity
8. Cancellation stops that client thread without cancelling accepted service
work. Bound client-runtime shutdown, including blocking DNS helper cleanup.
Use an owned terminal restoration guard on normal return and errors; Ratatui
also installs its panic restoration hook. A forced process kill cannot run Drop.

The client renders authoritative task snapshots and completed durable events.
It does not infer success from model text and does not display opaque reasoning.
It retains at most 256 recent output lines with 512-byte previews, 16 status
messages, and 512-byte form fields. Full journals stay on the host. Sanitize
terminal controls and escape grant paths/argv before rendering. Do not put
credentials in the terminal. Color supplements explicit state labels.

The coding form submits a host task path and model/account preference. Approval
review displays the frozen provider, account, model, root, writable files,
required changes, protected files, checks, argv, limits, and grant fingerprint.
It sends the displayed revision. Only the service can consume that revision.
Cancellation requires an explicit confirmation. q and Ctrl-C detach, including
while a task is active. A client disconnect does not imply cancellation.

Coalesce unchanged snapshots and task lists. Advance event cursors only after
handing the page to the bounded UI channel. Never drop a page containing new
events to improve display efficiency. Redraw on observations, input, and resize;
idle polling does not continuously write to the terminal. Re-read the private
connection file to follow a restarted daemon. Reconnect performs reads only.
A lost command response is reported with its identity and is never blindly
resubmitted. Keep recent status so reconnect cannot hide an uncertain identity.

## Evidence

Read [Ratatui 0.30.2](https://docs.rs/ratatui/0.30.2/ratatui/), its cached terminal
lifecycle and layout source, and [Crossterm 0.29 event rules](https://docs.rs/crossterm/0.29.0/crossterm/event/index.html)
before adoption. See [M4 evidence](../m4-evidence.md) for TestBackend cases and
real pseudo-terminal checks. Native provider qualification remains separate.

## Consequences and reversal

The first TUI is a local operator client with recent output, not a complete
transcript editor or token-stream display. Terminal rendering is qualified on
the development host only. Screen-reader, contrast, other terminal emulators,
and other OS qualification remain open. Another renderer can consume the same
client contract without changing execution or approval semantics.
