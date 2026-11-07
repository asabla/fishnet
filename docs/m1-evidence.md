# M1 acceptance evidence

Status: complete locally on 2026-11-07. M2 has not started.

## Delivered behavior

The two-package Rust workspace runs the scripted provider through the core-owned
model/tool/result/model loop. `fixture.lookup` reads an immutable in-process
catalog. It has no filesystem, process, network, or write capability. The CLI
has `run` and `inspect` commands and writes versioned JSON.

The runtime owns task transitions, operation correlation, attempt records,
monotonic durations, limits, cancellation, independent checks, and incident
selection. Durable journals preserve failed, cancelled, and unfinished runs.
Inspection validates saved records without invoking a provider or tool. The
model's final text cannot prove success. No memory or skill is generated.

## Commands actually run

On Darwin ARM64, macOS 27.2, with rustc 1.94.1 and Cargo 1.94.1:

```sh
./scripts/check.sh
```

This passed each command below:

```sh
cargo fmt --all -- --check
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo doc --workspace --no-deps --offline --locked
```

All 25 behavioral tests passed: 4 contract tests, 17 runtime/storage tests,
and 4 CLI process tests. The same script passed in a clean `git archive` export
of implementation commit `72b6359`, with a new target directory and the existing
Cargo dependency cache. No provider credentials or network access were required.
The preceding core-only commits were also checked in separate source trees.

These CLI commands also ran successfully:

```sh
cargo run --offline --locked -p fishnet-cli -- run
cargo run --offline --locked -p fishnet-cli -- inspect 1794066877930791000-67962-0
```

The local acceptance run saved 11 events, 2 provider attempts, 1 logical tool
call, 1 tool result, and a passed independent check. Its terminal state was
`verified_success`. The separate inspection process left the journal byte-for-byte
unchanged. The local record is in the ignored `.fishnet/runs/` directory.

The built binary also ran `unverified`, `check-failure`, `repeated-invalid`,
`negative`, and `provider-failure`. Their exit codes were 3, 2, 3, 0, and 2.
Their records remain in the local store.

## Acceptance checks

| Requirement | Observed evidence |
| --- | --- |
| Owned complete loop | Script asserts the actual preceding tool result; success has two model operations and one tool operation |
| Independent success | Contradictory evidence fails; a final success claim without evidence stays unverified |
| Cancellation | Before start, provider start, tool start, tool result, check result, and waiting-provider cases pass |
| CLI cancellation | SIGINT during a provider wait produces a saved `cancelled` outcome, no pending operations, and exit 130 |
| Deadlines and budgets | Immediate expiry, expiry during fixture wait, and provider-call exhaustion remain failed records |
| Bounded output | Oversized model/tool output fails; UTF-8 prefixes and oversized error diagnostics remain inspectable |
| Execution policy | Unknown tools are denied; malformed arguments are rejected; no shell or network tool exists |
| Restart inspection | Saved prefixes before a tool result and after a completed tool remain incomplete and unchanged |
| Process death | CLI is killed during its second provider operation; new-process inspection retains the completed tool result and pending provider operation |
| Record integrity | Corrupt complete lines, unsupported schema versions, and path-shaped IDs are rejected; partial final lines are reported |
| Repeated invalid calls | Equivalent parsed requests with different whitespace/field order emit an incident against fixed relevant state |
| Useful negative evidence | A missing catalog entry passes the negative fixture contract without a transport error or incident |
| Recovery after repetition | A corrected request can pass the original contract while both rejections and the incident remain in the record |
| Storage failure | An unusable store prevents provider execution; no command is accepted before durable creation |

The SIGINT test uses `/bin/kill` as OS test infrastructure on Unix. It does not
add a process execution capability to fishnet.

## Code quality review

The pinned formatter, compiler warnings, strict Clippy groups and selected
restriction checks, and rustdoc checks pass. There are no lint suppressions.
The [development policy](development.md) records the rules and local thresholds.

Review covered the boundaries between fixture scripts, authorization, tool
execution, evidence checks, journal writes, and replay validation. Each function
owns one operation at its boundary. Statement groups keep operations next to
error handling. Replay validation has no execution capability. Storage errors
propagate before further work. Cancellation state has one shared owner interface;
task state and journal writes have one runtime owner. No helper was added only
to reduce a metric. No readability exception remains.

## Limits of this evidence

- This is a deterministic fixture harness. It does not establish live model,
  provider, process sandbox, streaming, or strategy quality.
- Local process restart and partial-write cases pass. Power loss, filesystem
  fault injection during sync, and non-Unix directory durability are unverified.
- Deadlines and cancellation are cooperative. Fixture waits are interruptible.
  A blocked filesystem call, stdout write, or custom observer can delay handling.
- A storage error can leave an incomplete journal without a terminal outcome.
  Inspection reports uncertainty and never retries work.
- Retention is manual. Each record is bounded; the number of records is not.
  The local store is trusted and has no authentication or tamper-proof guarantee.
- The dependency cache and pinned toolchain must be available for offline builds.
  Initial dependency/toolchain installation can require network access.
- CI configuration runs the same checks on Linux and macOS. Hosted CI has not run.
  Linux, Windows, and other host configurations remain unverified.

## Next milestone

M2 selects one direct API provider and adds controlled tool execution. It must
cover native provider content, streaming, usage, effective argument checks,
explicit execution roots, and uncertain side effects. Live checks stay opt-in.
No M2 provider, shell tool, or execution runtime is included in this milestone.
