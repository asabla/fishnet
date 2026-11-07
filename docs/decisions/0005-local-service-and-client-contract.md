# ADR-0005: Durable local service and bounded client transport

Status: accepted

Date: 2026-11-07

## Problem and constraints

The user requested continued implementation through a working TUI. Complete M3
before M4. A client must not own accepted work, repeat effects after reconnect,
or grant itself new tool capabilities. Default tests must stay offline.

## Decision

Add one `fishnet-service` package under `services/daemon`, with a library,
`fishnetd` binary, and thin CLI transport. Keep execution in `fishnet-core`.
Use pinned Axum 0.8.8 with HTTP/1, JSON, query, and Tokio features. Reuse pinned
Reqwest and Tokio. Add subtle 2.6.1 for bearer comparison. Do not add a database,
external runtime, web UI, or another agent engine.

Use version 1 JSON envelopes over authenticated loopback HTTP. Clients use
explicit submissions, snapshots, cancellation, approval decisions, and bounded
event pages. Long polling is bounded to 15 seconds. It avoids an additional
stream protocol and keeps reconnection independent from connection lifetime.
The transport delivers completed durable events, not transient model tokens.

The host publishes a private connection file with an opaque capability. The
capability authorizes one local operator's reads and commands; this is not a
multi-user authorization system. Listeners reject non-loopback addresses.
Reject browser Origin headers. A protected tunnel can carry a remote client;
public TLS hosting and browser access need separate qualification. Redirects
and automatic client retries are disabled. Credentials never appear in URLs.

Persist a receipt before acknowledging acceptance. A stable request ID and the
typed request fingerprint implement idempotency. Reusing an ID with another
request conflicts. Never evict an idempotency key automatically. Limit each
service store to 128 receipts. Archive stores explicitly when needed.

The core accepts a reserved run identity and uses create-new journal semantics.
A duplicate identity cannot replace a journal. On restart, inspect accepted or
executing receipts and their journals. Report finished work or an interrupted,
unknown outcome. Never resume or replay it. Preserve incomplete local metadata
writes in a private incidents directory without applying their contents.

Coding is disabled unless the host enables it. Load reviewed task files from
`tasks/` under the fixed host root. Capture the task, resolved argv, and provider
configuration before approval. A client cannot send arbitrary executable argv
or replace the in-memory grants. An approval compares the pending revision,
persists the decision, and transfers one execution slot. Other decisions at
that revision conflict. New tasks measure approval wait with a monotonic clock;
wait across service restart remains unknown.

Mutating commands have owned asynchronous lifetimes. Abandoning an HTTP response
cannot interrupt a durable acceptance-to-execution handoff. Admission is bounded:
4 active tasks, 16 mutating commands, 16 HTTP requests, and 8 event long polls.
Only one active task can be a coding task. Coding grants can share paths, so
concurrent execution could overwrite another task's edits. Additional approvals
remain pending. This rule is local to one service; use one service host per
working tree and do not run a standalone writer against that tree concurrently.
There is no per-consumer event queue. Each consumer pages the existing journal.
At most 8 events are returned per page. The live window retains 128 events;
a gap returns an explicit flag, current snapshot, and recovery cursor. Full
bounded journals remain available to local inspection. Client responses have a
1 MiB bound. A full or slow consumer cannot block journal publication.

`fishnet serve` and `fishnet task` expose the common service contract. The
standalone M1 fixture and M2 coding commands remain explicit compatibility
utilities. Their process ownership does not gain daemon guarantees. CLI and
TUI service commands use the same client library and core execution semantics.

## Evidence

Read [Axum 0.8.8](https://docs.rs/axum/0.8.8/axum/) and its cached source before
adoption. See [M3 evidence](../m3-evidence.md) for protocol, crash-boundary,
headless client, and real loopback checks. No compatibility claim follows from
manifest parsing or a dependency build alone.

## Consequences and reversal

The service is a single trusted local operator boundary. Private Unix files are
not encrypted at rest. Local process execution remains subject to the M2
trusted-project limitation. Store-wide retention is manual. Polling re-reads
bounded records; no throughput claim is made. A future SSE or database adapter
must preserve identity, approval, gap, and unknown-effect semantics.
