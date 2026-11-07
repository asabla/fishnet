# M3 acceptance evidence

Status: complete on the local macOS ARM64 host, 2026-11-07. The user requested
continued work through M4. M3 was implemented and checked before starting M4.

## Implementation

The `fishnet-service` package owns accepted tasks independently of clients.
`fishnetd`, `fishnet serve`, and the `fishnet task` client use the version 1
contract in `services/daemon/src/contract.rs`. The existing Rust core still
owns provider/tool execution, policy, diagnostics, and journals.

Durable receipts record stable request IDs, fingerprints, prepared host grants,
provider settings, approval decisions, and approval-wait evidence. A duplicate
submission returns its original task. A conflicting payload returns a conflict.
Approval uses a durable compare-and-set revision. Accepted mutations have a
bounded owned lifetime, so losing the response cannot abandon a handoff.

Authentication applies to commands, snapshots, lists, and event pages. Connection
files and bearer tokens use private Unix permissions. Listeners bind only to
loopback. Coding is disabled until the host enables it, and task files must be
under the host's `tasks/` directory. Requests cannot replace effective argv.

Consumers use snapshots and bounded event pages. Reconnect from a sequence
cursor without resubmitting work. Retention gaps return a snapshot and recovery
cursor. Slow readers hold no event queue and cannot block runtime publication.
A service restart inspects the journal and marks interrupted work explicitly.
It does not replay providers or tools. Incomplete local metadata writes are
preserved in private incidents, without applying them or generating memory.

## Checks actually run

`make check` passed: formatting, 60 offline tests, Clippy for all targets with
warnings denied, and rustdoc with warnings denied. `make build` also passed.
No default test requires a network connection or account credentials.

The service adds one retained-gap unit case and eight integration cases:

- Two concurrent submissions with one ID produce one journal and one tool call.
  Changed input with that ID conflicts. The ID survives restart.
- A headless in-process HTTP client detaches, reconnects from cursors, and sees
  the verified terminal result without lost or repeated events.
- Unauthorized reads and writes fail. Wrong protocol versions and browser
  origins are rejected before task acceptance.
- Concurrent approval decisions consume a revision once. Changing the host task
  file after submission cannot replace the frozen grant. Denial stays durable.
- The approval-to-execution path is tested with a deliberately unavailable test
  API key. It fails before network access and does not execute twice.
- Four active tasks bound execution. A fifth is rejected before acceptance.
  Explicit cancellation produces core cancellation records.
- Eight held long polls bound event consumers. A ninth is rejected, while a
  cancellation request can still enter and complete.
- Restart after a persisted start and partial metadata write preserves unknown
  pending work and archives the partial write. It does not replay execution.
- Duplicate service ownership and loose credential-file permissions fail.

The gap fixture produces more than 128 real core events. It checks explicit gap
reporting, authoritative snapshot statistics, retained recovery, and an invalid
future cursor. Existing M1 and M2 cancellation, edit-boundary, stream, and
independent-outcome checks remain in the default suite.

## Real process qualification

Started the built daemon on an ephemeral loopback port with isolated local
stores `.fishnet/m3-smoke` and `.fishnet/m3-smoke-runs`. Used the built CLI from a
separate process to submit the offline `success` fixture with request ID
`m3-cli-proof` and a 100 millisecond fixture delay.

Task ID: `298b1b9518fb4ad5dfe974d23a1f43af3a4e98c08499c06c151202850a55b16e`.
The event client received 11 durable events and a terminal snapshot with verified
success, 2 provider attempts, and 1 logical tool call. Stopped and restarted the
daemon. Repeated the same submission. It returned that original task, with the
same journal SHA-256 and tool count. No provider or tool was replayed.

Loopback listener and client permissions were enabled explicitly in the Codex
execution environment. This qualification used no external provider request or
account secret. Live provider behavior remains the separate M2 evidence.

## Limits and next step

The service has one operator capability, 128 durable receipts, 4 active tasks,
16 owned mutating commands, 16 HTTP requests, and 8 event long polls. Event pages
contain at most 8 events from a live window of 128; full bounded journals remain
on disk. Store-wide retention and archival are manual. Record paging uses
bounded file reads and has no throughput qualification.

Public listeners, browser clients, multi-user roles, TLS termination, platform
credential encryption, and automatic crash resumption are not implemented.
Host commands retain the M2 trusted-project and OS-permission boundary.
Standalone M1/M2 CLI commands remain compatibility utilities; service ownership
and reconnect guarantees apply to `fishnet task` commands.

Next: M4, the TUI client of this service.
