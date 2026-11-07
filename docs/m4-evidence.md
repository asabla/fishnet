# M4 acceptance evidence

Status: active usability revision, 2026-11-07. Initial technical acceptance on the
local macOS ARM64 host is retained below. It did not establish a suitable user
interface. The latest conversation revision is recorded at the end. M5 remains pending.

The sections before "Usability revision after user review" describe the initial
M4 build and remain historical evidence. Use the revision below and current usage
guides for current behavior and qualification limits. The conversation revision
at the end replaces the earlier letter shortcuts and dashboard. Technical milestone
acceptance did not establish product readiness.

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

## Usability revision after user review

The initial technical acceptance did not establish a usable normal workflow.
The user rejected the two-terminal startup, command syntax, and crude rendering.
The revision stays in M4; it does not start M5.

Delivered:

- The installed `fishnet` command opens the interface and starts or reuses its
  working tree's local service. Headless no-argument use prints help without
  creating service/account files. Explicit custom connections only attach.
- `make run` opens the app. `make install` installs the single command offline.
  `make fixture` preserves the offline JSON fixture utility.
- n describes a Rust task and exact files. The host freezes Cargo fmt, test,
  Clippy, and documentation checks. Enter opens full grant review without a
  provider request. Advanced reviewed task files remain available with p.
- Activity wraps. Task states have complete labels. Task selection adapts to
  width. Modal backgrounds clear unrelated content; help and review scroll and
  retain their controls. Text fields support cursor editing and bounded paste.
  Detailed identities, counts, measured timings, and uncertain statuses use v.
- q waits for outstanding command acknowledgements. Ctrl-C detaches immediately
  and prints pending/uncertain identities after restoring the terminal. Neither
  path retries a command. Known pre-core provider initialization failures are
  labelled failed; uncertain interrupted execution remains unknown.
- Task and account output is readable by default, with explicit `--json` for
  scripts. Task commands accept unique ID prefixes. `fishnet stop` requests
  authenticated cancellation and shutdown. Upgrade client and host together.

The current full quality gate passed with 81 offline tests, formatting, strict
Clippy, and strict rustdoc. A separate Git export of
`1138eebc9f157e126eacfbcea2980738e6dc0905` passed the same 81 tests and quality
checks, with no account/run files and token, API-key, and secret environment
variables removed. The export used the populated Cargo cache and shared build
directory; it does not qualify an uncached installation. The eight retained row snapshots cover welcome,
activity at normal/wide sizes, task input at 80x24 and 40x15, approval start/end,
and help end. Additional assertions check wrapped output scrolling, 40x12 grant
controls, Unicode cursor edits, complete outcome labels, and acknowledgement
behavior. Snapshots are under `apps/tui/tests/screens/`; an update requires
explicit `FISHNET_UPDATE_SNAPSHOTS=1` and review of the resulting rows.

### Real process checks

Ran the debug and installed release commands against a disposable Rust project.
No fishnet repository file was placed in that provider-readable root. Retained
local evidence is under `.fishnet/m4-usability`; it contains PTY transcripts,
run journals, pre-live grant receipts, and verification logs. No account or
connection credential file was copied into that evidence directory.

- A single command started its private loopback service and opened the interface.
- Fixture `502ef3f24e40d754fa2ac8da5c1677d026f425d5290f6ffc9dc3e953a9a7543e`
  rendered verified success after its independent check.
- The new-task form prepared grant
  `84127b76824f7ab1aa71850dac026b6a58a49b2556104fa1e2c0d0df5f3474c1`.
  Rejection persisted and made no provider request.
- In the installed command, f followed immediately by q displayed the wait for
  service acknowledgement, then detached. Fixture
  `686cf430c1b58f958649a5b96304668bc72025a40bc8c87d3ea01701c878fbb9`
  finished under service ownership. Each fixture retained one logical tool call.
- The installed CLI displayed readable task rows and resolved 12-character
  prefixes. Explicit JSON contained the full states and counts. Authenticated
  shutdown completed.
- Starting `fishnet attach` with a prefix restarted the service and reattached in
  a 40x15 PTY. Both fixture journal SHA-256 values and the frozen pending grant
  were unchanged. Approval wait across restart remained unknown. No operation
  was replayed.
- Grant review scrolled to its final disclosure in the small PTY while approval,
  rejection, and close controls remained visible. Normal exit restored the
  alternate screen. Color-enabled and NO_COLOR output were exercised.

### Cancellation verification failure and correction

The first source-export check at `a7f435c` failed the existing process-group
cancellation test: its delayed child marker existed. Six isolated reruns passed;
that did not establish a fix. The old test announced readiness in the parent and
relied on the writer sleeping until cancellation. It could not distinguish a
surviving child from a write that occurred before cancellation was scheduled.

The revised fixture announces readiness in the actual child writer and waits
for a release file that the test creates only after execution returns. Six gated
trials passed. A controlled mutation that disabled process-group termination
failed on the late marker as expected. Production termination source was
restored unchanged. The full 81-test gate then passed. The initial failed export,
original reruns, gated trials, and expected mutation failure remain separate
verification records; they were not removed from the evidence.

### Approved live coding qualification

After explicit user approval on 2026-11-07, task
`5e37a8a7bace07e02096e70909e56f83df81a39ae56afc82013c109ecc2a710b`
completed through the installed TUI using the ChatGPT account and model
`gpt-6.1-sol`. The frozen grant fingerprint remained
`bc63bcd55665a22fa493c9afd67d58aba5d3fb838f58ec3d5ba91b8ed51b057e`.
Only the disposable project's `src/lib.rs` was writable. No fishnet repository
file was present in its readable root. Credentials were excluded from task
content and retained evidence.

The TUI approved revision 1. It detached while the task was running, and a
separate CLI confirmed that execution remained service-owned. Reattachment
showed the durable result and all independent checks. The function changed from
returning zero to `left + right`. Source inventory and SHA-256 comparisons showed
that only `src/lib.rs` changed; the existing addition test, Cargo manifest, lock,
and toolchain file stayed unchanged.

The model used read_file, replace_text, and the four granted Cargo commands.
Its only file-read tool call read `src/lib.rs`. After its final response at
sequence 28, the host independently ran fmt, tests, Clippy with warnings denied,
and documentation checks. They finished at sequences 30, 32, 34, and 36, each
with exit code zero, passed status, unchanged protected files, and no output
truncation. The existing `adds_two_numbers` behavioral test passed. The terminal
outcome at sequence 37 was verified success, based on these checks and the
required source change.

The record retained 7 model requests, 6 logical tool calls, and 8 process
attempts: four model-selected commands and four independent host checks. These
counts describe this trial; they are not the success criterion. Service restart
preserved the outcome, all 37 events, operation statistics, and journal SHA-256.
No provider or tool operation repeated. The validation TUI and service were
stopped after inspection.

Local evidence under `.fishnet/m4-usability` includes `live-record.json`,
`live-audit.json`, before/after restart views, baseline source hashes, the final
receipt and source, the JSONL journal, and three PTY transcripts. The earlier M2
self-development result remains separate evidence. This live trial qualifies
the initial interactive coding workflow for this project shape and account; it
does not qualify arbitrary coding tasks or providers.

### Qualification boundary

Native Terminal access was blocked by the computer-use tool. PTY behavior and
headless screen rows are verified; actual rendering in that app, other terminal
emulators, accessibility certification, Linux/Windows, and hosted CI are not.
Commands still use host OS permissions. Guided Rust checks establish their
listed criteria, not every behavioral claim about an objective.

## Conversation revision after screenshot review

The user rejected the dashboard and requested actual Codex and OpenCode TUI
screenshots. Browser inspection of the official reference pages preceded this
revision. [ADR-0008](decisions/0008-conversation-prompt-and-transcript.md) records
the decision and its limits. This is the first correction, not product parity.

Delivered:

- Ordinary letters enter text in a persistent bottom prompt. The prompt accepts
  multiline paste and cursor edits. Slash commands replace global letter
  shortcuts. A command menu supports unique-prefix completion. Startup leaves
  saved tasks behind /tasks instead of silently opening one.
- One transcript column separates user intent, model text, tool activity,
  independent checks, errors, and the authoritative outcome. Tool/check results
  join their operation entries. Provider request noise remains in /details.
  Replayed sequences do not duplicate entries. Unsent text survives selection
  and late acknowledgements. The model message cannot establish success.
- File access follows intent entry. The frozen host grant and its revision are
  still reviewed before execution. Full-width temporary panels replace floating
  forms. Small-screen review controls remain visible. The journal and execution
  policy are unchanged. No dependency was added.

Checks actually run on the local macOS ARM64 host:

- `./scripts/check.sh` passed with **90 offline tests**, formatting, strict
  workspace/all-target Clippy, and strict rustdoc. `cargo build --workspace
  --offline --locked` also passed. Default tests made no live provider request.
- Ten retained text snapshots cover the conversation, welcome screen, file
  scope, grant start/end, help end, multiline prompt, and command menu. Tests
  cover Unicode edits, bounded/sanitized paste, exact-column cursor wrapping,
  draft retention, operation grouping, replay, task isolation, and explicit
  terminal outcomes. The exact-column case first failed because the word
  wrapper discarded the space used for the cursor. A visible caret fixes the
  lost cursor, and the regression now passes.
- Browser visual review used exported **actual TestBackend cell symbols and
  styles**, including 80x24 conversation, 40x15 file access and command menu,
  and 40x12 grant controls. The retained HTML and SVG preview use synthetic
  layout content. They are renderer evidence, not native terminal screenshots.
- An isolated real 80x24 PTY ran the new input flow against the Rust service.
  Ordinary letters did not detach or invoke tools. /fixture reached verified
  success for task
  `226f824b0580df3c8d438e30eb4b38d782251a7d4ec813a4d73cd76ff68d9b51`.
  Its independent check passed; it retained two scripted model requests and one
  logical tool call. These counts are diagnostics, not the success criterion.
- Typed intent and a named file prepared a complete Rust grant. Rejection of
  task `7e5844b858860b9f5fc3a6410e7ee1df6b87e24a697eefbdacdb5f88ef206f4a`
  created no coding run. The disposable source file remained unchanged. The
  service used a fresh empty account directory. No real provider was contacted.
- /quit restored terminal settings. After service restart, reattachment in a
  40x15 PTY displayed the same verified outcome and opened /details. Run state,
  statistics, and journal SHA-256 were unchanged. No tool was replayed. Both
  PTYs restored their previous terminal settings; test hosts were stopped.

The PTY test setup first rejected a noncanonical temporary-directory path. Its
runner also first waited for client exit without draining output and read a
stale connection file during restart. The corrected runner resolves its root,
continuously drains terminal output, and waits for new connection data. The
successful result above comes from that corrected run. These setup failures do
not establish product failures or substitute for the successful assertions.

Local evidence is under `.fishnet/m4-conversation`: quality/build logs, the PTY
runner and receipts, PTY transcripts, and renderer previews. Connection files
and credentials were not copied there. No memory or skill was generated.

M4 remains active. The service still accepts separate tasks; prior text is not
conversation context for a new prompt. Persistent conversational turns,
transient text streaming, rich Markdown, and inline diff presentation are next.
Native terminal appearance, other terminal emulators, accessibility, other OSs,
and hosted CI remain unqualified. The earlier approved live coding task used
the previous interface and is not new live evidence for this revision. M5 has
not started.

## OpenCode-inspired visual refinement

Baseline: the previous revision was committed as `b2ef1d3` and `412c947`.
The working tree was clean before this pass. Browser review used the official
[OpenCode terminal page](https://dev.opencode.ai/terminal) as the visual reference.

The start screen now centres an original fishnet wordmark above the composer.
The composer has a narrow blue edge, a request-model row, and a small project
and shortcut row. Its width is limited to 80 cells on the start screen. Active
tasks use a column limited to 100 cells and keep the composer at the bottom.
Tool blocks have quieter backgrounds, human-readable action titles, and fixed
text gutters that remain correct when content wraps or scrolls. Temporary panels
use the same surface. The narrow task picker retains its navigation controls.

The model row shows the configured request preference or "host default". It does
not claim a provider or observed model when neither is known. Routine connection
messages remain available in details. Attention notices and uncertain-command
counts stay visible across reconnect. No provider, policy, grant, execution, or
journal contract changed. No dependency or lint exception was added.

Checks actually run:

- `./scripts/check.sh` passed with **93 offline tests**, formatting, strict
  workspace/all-target Clippy, and strict rustdoc. `cargo build --workspace
  --offline --locked` passed.
- Fifteen retained row snapshots cover the start, active session, multiline
  prompt, command menu, file scope, grant review, help, and task picker. New
  assertions cover narrow task controls, request-model display, reconnect
  warnings, and cleared panel backgrounds. Renderer exports now use snapshot
  names so concurrent tests cannot overwrite another scene's preview.
- Browser colour review covered the start screen, 120x30 and 80x24 active
  sessions, retained SVG images, and the 40x15 task picker. The images contain
  synthetic layout data and use actual TestBackend symbols and styles.
- The isolated real PTY flow passed at 80x24 and reattached at 40x15. Fixture
  `f097c3ae6567ea4fd0bc9ac05789110986354977763040e8c312c3740520a4bc`
  reached verified success after its independent check. Rejected coding grant
  `7e007f211fc3832f28f39d6e15eccd19a5662dbe785381000ea8dc1d23807d7c`
  started no coding run and left the disposable source unchanged. No real
  provider was contacted. Detach restored terminal settings. Restart preserved
  run state, statistics, and the journal SHA-256 without operation replay.

`make install` passed and replaced the installed command. The installed
`fishnet tui --help` also returned successfully without starting a service.

Local evidence is under `.fishnet/m4-opencode`. It includes check/build logs,
the isolated PTY driver and result, terminal output, named HTML exports, and
SVG images. Preview and fixture services were stopped after review.

This remains M4 work. Conversational turns, transient streaming, rich Markdown,
and inline diffs are still pending. Browser cell previews and PTYs do not
establish native terminal appearance, accessibility, other OS support, or hosted
CI qualification. The earlier live coding trial remains separate evidence.

## Launch-path and terminal-colour correction

The user reported that actual launches did not resemble the previews. The
installed and checkout binaries contained the new layout. Capturing their real
terminal output found a verification gap: validation inherited `NO_COLOR=1`,
while the TestBackend previews retained RGB styles. The terminal stream emitted
empty combined colour sequences, so it lost the preview's shaded surfaces.
Earlier text/lifecycle checks and colour previews did not establish actual
terminal colour output. The earlier "colour-enabled" claim is not sufficient
colour-encoding evidence. Also, `fishnet run` still selected the JSON fixture.

[ADR-0009](decisions/0009-launch-and-terminal-colour.md) records the correction.
`fishnet run` is now an interactive alias. The standalone utility is named
`fishnet fixture`, and `make fixture` uses that name. The fixture loop and run
records are unchanged. The existing bootstrap inspection check now calls the
renamed fixture command; its outcome, count, size, and no-replay assertions are
unchanged. The first full check failed at that stale invocation. Its log remains
in `.fishnet/m4-launch-diagnosis/check.log`; the corrected check passed separately.

`--color auto` respects nonempty NO_COLOR. Monochrome mode uses reverse-video
surfaces and labels the mode in the footer. `--color always` enables the colour
theme explicitly; `--color never` selects monochrome explicitly. The application
does not change environment variables.

Checks actually run:

- `./scripts/check.sh` passed with **95 tests**, formatting, strict Clippy, and
  strict rustdoc. One helper test is marked ignored for direct harness execution;
  the parent test invokes it in four isolated processes for all colour-policy
  cases. No colour case is skipped by the default parent test.
- The backend test draws actual cells through Crossterm into terminal bytes.
  It checks RGB accents when enabled and reverse video when disabled. It rejects
  empty combined colour sequences. These checks need no interactive terminal,
  network, or account. A retained monochrome row snapshot uses the same resolved
  appearance as terminal drawing.
- Fresh 80x24 PTYs verified the installed command, checkout command, run alias,
  and `make run`, all with `--color always` and NO_COLOR set. Each emitted the
  new prompt, model row, wordmark, and RGB accent sequence. A separate installed
  auto-mode run emitted reverse video and no RGB. All exited normally.
- The colour-enabled fixture/reject/detach/restart flow also passed. Fixture
  `a2c463a7c1456916e0a165ce2975910e5224283ec6f4b675559a36d9f326216c`
  reached independently verified success. Rejected coding grant
  `c02453317f5591e0241d2005da9fce0e47349772e00f9c99ffb9c8abc916a461`
  started no coding run. Source and journal state remained unchanged across
  inspection/restart; terminal settings were restored. No real provider was used.
- `make install` replaced the installed command with the correction.

Evidence is under `.fishnet/m4-launch-diagnosis`: the initial diagnostic result,
corrected launch results and terminal captures, full-check logs, and isolated
fixture receipts. These verify routing, terminal encoding, and lifecycle. The
user's specific native terminal appearance still requires their retest; no
native screenshot is claimed. M4 remains active and M5 is pending.
