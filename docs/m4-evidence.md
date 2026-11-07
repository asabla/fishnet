# M4 acceptance evidence

Status: complete on the local macOS ARM64 host, 2026-11-07. M3 was committed and
verified before the terminal client was implemented. M5 remains pending.

## Delivered behavior

`fishnet-tui` is a client of the common service API. `make tui` opens task
selection, recent output, state and counts, an explicit coding form, approval
review, cancellation confirmation, and status/help views. q and Ctrl-C detach.
The service remains the owner of work. CLI and TUI share the same client library,
approval revision, task identity, snapshots, and journal event cursors.

The UI distinguishes verified success, unverified completion, failure,
cancellation, rejection, pending approval, and interrupted/unknown work. A model
final message is labelled as still requiring checks. A late response cannot
replace another task's view. Grant paths and argv are escaped, terminal control
characters are sanitized, and opaque provider reasoning is not displayed.

Input and terminal drawing stay on one thread. A separate bounded client worker
performs HTTP reads and explicit commands. Unchanged observations are coalesced;
new durable events are delivered before advancing the cursor. An idle PTY poll
produced no redraw bytes. A bounded recent-status view retains uncertain command
identities across reconnect. No command is automatically resubmitted.

A final grant review also found that read-only task profiles advertised a write
schema with no permitted paths. The schema now omits that tool. A regression
checks both advertised capabilities and actual denial of a direct write call.
Execution permission was not expanded.

The service also admits only one coding task at a time. A regression holds the
post-approval execution slot, then checks that a second approval is rejected for
capacity without consuming its pending grant or starting a provider. Read-only
fixtures can still use the other slots.

## Checks actually run

`make check` passed with 72 offline tests, formatting, Clippy for all targets
with warnings denied, and strict rustdoc. `make build` passed. The default tests
need no network, credentials, or interactive terminal. `make tui ARGS="--help"`
was also exercised without a terminal or provider request.

Repeated `make check` from a separate Git source export of
`9da186f266fb72f862ddbb60cd90e0bb06a21fb5`. All 72 tests and the same quality
checks passed. The export had no `.fishnet` account or run files, and token,
API-key, and secret environment variables were removed. It used the populated
Cargo cache and shared build directory; this does not prove an uncached install.

The 10 TUI TestBackend/input cases cover model-message versus checked outcome,
all terminal labels, detach versus cancellation, current approval revisions,
quoted grant content, bounded paste, terminal-control removal, output bounds,
stale responses, explicit gap recovery, task selection, key releases, narrow
layouts, and status history after an uncertain command and reconnect. Layouts
were rendered at 120x36, 80x24, 60x15, 40x10, and 0x0. Existing core and service
cases remain in the default workspace suite.

## Real terminal and process evidence

Ran the built TUI in a 120-column, 32-row pseudo-terminal against an isolated
real loopback daemon. Private local evidence is under `.fishnet/m4-smoke`; run
journals are under `.fishnet/m4-smoke-runs`. The daemon's test API-key variable
was explicitly unset, so no live provider or source transfer was possible.

- Pressed n. Task `574fd2462e5da5b56d4f29604555e77289c8175f70c4d2510309a00e4bd4ed35`
  ran the fixture and rendered verified success after its independent check.
- Submitted a host coding task, opened and scrolled its complete grant, then
  rejected it. Task `6c875280d81cfe0cd2d7b35d49dc97c8acfd1328e27f26d38de82b5287120e4e`
  persisted rejection and created no run journal.
- Submitted the same host task under a new identity and approved its grant.
  Task `ec8a9ff647c4d67124a0dc2eadf239e4f1e16da29bdc349dbb1f48095eac849`
  recorded approval, then reported the deliberately missing credential before
  provider access. It created no core journal and made no source edit. The UI
  showed the interrupted state and credential error, not verified completion.
- Attached to delayed fixture
  `a135d61821a8a69da6c1250a8f5dfa6b6f19fa770f0216cfba03307554343d4c`,
  detached with q, and used a separate CLI process to confirm it remained
  running. Reattached and cancelled with c then y. Both clients observed
  cancellation, with one provider attempt and zero logical tool calls.
- Left the TUI open, stopped the daemon, and observed disconnected status.
  Restarted the daemon. The TUI reconnected without user resubmission. A separate
  CLI agreed on the cancelled outcome. The journal SHA-256 and operation counts
  were unchanged across restart.
- Normal exit restored the alternate screen. Saved `stty -g` settings were
  byte-identical before and after the transcript run. All validation clients and
  daemons were stopped after the checks.

The inspection source SHA-256 remained
`0f64aed609a93b760900e3b2898c851e5f672af4dac5a12b28e4596cdfa5eee8`.
No live coding task was sent through the TUI during this qualification. The
existing M2 ChatGPT coding result is separate evidence, not a new TUI live claim.
Failures and cancellations remain in their service stores; no memory or skill
was generated from them.

## Limits and next milestone

The UI keeps 256 recent lines with 512-byte previews, 16 status entries, and
512-byte form fields. Commands and observations use channels of capacity 8.
Full history remains in bounded host journals. Output consists of completed
durable events; transient token display is not implemented.

The M3 single-operator, loopback/tunnel, private-file, capacity, and manual
retention limits remain. Approved commands use host OS permissions; there is no
OS sandbox. Use one service host per working tree; separate hosts and standalone
commands do not share the coding execution lock. Live coding through the TUI,
other terminal emulators, Linux/Windows, accessibility certification, and hosted
CI are not qualified by these checks.

The next planned milestone is M5: plugins, skills, and middleware. It was not
started. See [TUI usage](../apps/tui/README.md) and the current development guide.
