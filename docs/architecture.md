# Architecture proposal

Status: proposal. Record selected implementations in decision records.

## Boundaries

Use one core execution model behind a service interface. CLI, TUI, and remote clients send commands and consume events. A transport adapter must not redefine task behavior.

| Component | Responsibility |
| --- | --- |
| Protocol | Commands, events, errors, capability negotiation, schema versions |
| Runtime | Task/session state machine, model/tool loop, cancellation, scheduling |
| Provider adapter | Streaming, tool calls, native content preservation, usage, provider capabilities |
| Tool executor | Typed arguments, policy checks, process lifecycle, output limits |
| Plugin host | Package validation, discovery, capability grants, hooks, middleware |
| Workspace | Roots, repositories, revisions, instruction scope, change sets |
| Context engine | Retrieval selection, token budgets, compaction, source recovery |
| Knowledge interfaces | Facts, evidence, wiki output, retrieval, optional graph relations |
| Storage | Durable transitions, snapshots, artifacts, retention |
| Diagnostics | Correlation, durations, counters, fingerprints, incident evidence |
| Evaluation | Fixtures, fresh trials, independent checks, policy comparison |

These are logical boundaries. Do not create all of them as crates at the start.

## Candidate infrastructure

Tokio, Axum/Tower, Clap, Ratatui/Crossterm, SQLite, rmcp, tracing/OpenTelemetry, and Tree-sitter are candidates from research. Verify current support and record a decision when a component is introduced. TypeScript workers, Python workers, embedded Lua, and Wasmtime remain alternatives for external middleware.

## Execution and persistence

A task has explicit states for queued, running, waiting for input, cancelling, and terminal outcomes. Terminal outcomes distinguish verified success, unverified completion, failure, and cancellation. Define transitions before implementing them; record an outcome's evidence.

Use one owner for mutable session state. Allow concurrent tools only when scheduling and write-conflict rules permit it. Attach task, turn, operation, attempt, and trace identifiers.

Keep durable state events separate from transient token output. Define a durability boundary before acknowledging accepted commands. A storage transaction can couple local event/state updates, but cannot make a database write and an external tool effect atomic.

On restart, reconcile incomplete operations. Rebuild state from records without re-executing external effects. A lost response after a write means the outcome may be unknown. Use service-supported idempotency keys or explicit reconciliation, not blind replay.

## Clients

Define a versioned command/event API. Consider HTTP commands and SSE events; use WebSockets where bidirectional streams are needed. Consider ACP and OpenAI-compatible adapters separately. Specify reconnect cursors, retained-event gaps, snapshot recovery, idempotent command submission, and multiple-client approval ownership.

Remote access needs authentication and authorization for commands, streams, and artifacts. Bind locally by default. Keep policy enforcement in the runtime/executor. UI declarations cannot grant permissions.

## Context and knowledge

Memory storage, retrieval, context selection, and wiki publication are separate interfaces. Preserve original evidence and revisions. A summary or graph edge must not silently replace its source. Use a text-search baseline before claiming that graph retrieval improves results.
