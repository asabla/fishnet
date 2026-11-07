# ADR-0002: Durable JSON journals and independent fixture checks

Status: accepted

Date: 2026-11-07

## Problem and constraints

M1 must preserve failures and cancellations. Inspection after restart must not
execute tools. A model final message cannot prove success. Storage must remain
small and local.

## Options considered

An in-memory record loses evidence on restart. Replacing a full snapshot after
each operation rewrites all evidence. SQLite is suitable for future concurrent
queries and transactions, but M1 has one writer and bounded records.

## Decision

Create one append-only JSONL file per run. Open with `create_new`; use mode 0600
on Unix. Sync directory entries on Unix when creating the store and journal.
Write and sync each event before publishing it or starting the next operation.
The persisted operation start precedes execution. Storage errors halt execution.
Use schema version 1. Reject unsupported versions and invalid complete records.
No existing journal can be opened for execution or appended to after restart.

A reader takes a bounded file-size snapshot. It validates complete lines and
reports an incomplete final line as `partial_tail`. It leaves the file intact.
An unfinished operation remains pending, with no inferred result. `incomplete`
can mean an active run or a stopped process. Inspection cannot distinguish them.
It never constructs a provider, executes a tool, or resumes a task.

The state machine is `queued -> running -> verified_success`,
`unverified_completion`, or `failed`. Cancellation takes
`queued|running -> cancelling -> cancelled`. Terminal states cannot restart.
A deadline is a failure with `DeadlineExceeded`; a user cancellation is distinct.

The task contract and fixture expectations are set by the harness. The checker
compares persisted tool-result evidence with the contract. Expected values are
separate from the tool implementation. A missing result gives unverified
completion. A contrary result fails the check. A final model claim is only text.

The fixed `fixture.lookup` tool has an immutable catalog and no filesystem,
process, network, or write capability. Missing entries are expected negative
evidence. Unknown tools are policy denials. Malformed arguments are schema
errors. Equivalent rejected calls use parsed JSON equality plus the immutable
catalog, tool, and policy versions. Whitespace and object field order do not
change identity. Distinct values remain distinct. A repeat records an incident;
it does not replace the outcome check or prohibit recovery.

## Evidence and validation

Use [`File::sync_all`](https://doc.rust-lang.org/1.94.1/std/fs/struct.File.html#method.sync_all)
for each durable record boundary. The tests check interruption before a tool
result and after a completed tool, partial records, corruption, process death,
and unchanged files after inspection. See [M1 evidence](../m1-evidence.md).
These tests establish process restart behavior. They do not simulate power loss.

## Consequences and compatibility

Defaults: 16 provider calls, 4096 bytes per model/tool response, 5000 ms task
deadline. Hard bounds: 32 calls, 8192 response bytes, 300000 ms deadline, 256
events, 65536 bytes per serialized event, 16 MiB per inspected record. All
response bounds are in bytes, with UTF-8-safe diagnostic prefixes. An oversized
response fails; truncation cannot pass the completion check.

Durations use `Instant` within the process. Unix timestamps support correlation.
One M1 run is one session, task, and trace; their IDs coincide. Provider attempts,
logical tool operations, check operations, and incidents remain distinct.
Operation duration includes its start-record write, and excludes its finish-record
write and observer. Task duration includes completed child work. Timings overlap
and must not be added as if they were independent.

Observations and incident events share a run journal but have distinct types.
They are not user/project memory. No memories, skills, hypotheses, or policy
promotions are generated. Provider usage, cache, and resource measurements are
not collected in M1. Missing data is unknown. No run-count objective is used.

Retention is manual. No automatic cleanup removes failed runs. Records are
bounded per run, but the number of retained runs is not capped. The store is
trusted local data, not an authenticated or tamper-proof audit service. There
is no hostile-filesystem sandbox claim. Directory syncing outside Unix and
power-loss behavior remain unqualified.

## Reversal or migration path

Add a versioned reader when the schema changes. Import complete journal events
into a later embedded database without executing them. Keep uncertain operations
unresolved until an explicit reconciliation contract exists. Do not infer that
an interrupted external write can be retried when M2 adds execution capabilities.
