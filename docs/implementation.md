# Implementation plan

M1 through M4 are complete as of 2026-11-07 on the qualified local host. M5 through M7 are pending. Each stage produces a usable increment and linked acceptance evidence.

## M1: Deterministic local vertical slice

Status: complete. The CLI, owned Rust loop, fixture tool, durable records, and independent checks pass the local acceptance cases. See [commands and limitations](m1-evidence.md).

Goal: exercise the owned execution loop without network access or credentials.

- Create the minimal Rust workspace, CLI, core runtime, protocol types, and test fixture provider.
- Support a scripted model response that requests a deterministic read-only tool, receives its result, and returns a final response.
- Add explicit task states, operation IDs, cancellation, deadlines, bounded output, and structured JSON event output.
- Persist sufficient task/operation state for inspection after restart. Choose and record the minimal storage implementation.
- Record provider attempts, logical tool calls, results, errors, duration, and task outcome. Use monotonic time for durations.
- Add a minimal task contract and independent fixture outcome check. Distinguish verified success from unverified model completion.
- Detect equivalent repeated invalid tool requests against unchanged relevant state. An expected negative test result must not be labeled a tool transport failure.
- Keep execution tools restricted to fixture operations/read-only workspace access for this stage. Shell execution and network providers are later stages.

Acceptance: a clean checkout builds and tests without secrets; one CLI run emits correlated events and passes an independent check; a cancelled run emits a consistent terminal outcome; restarting permits inspection without rerunning tools; duplicate invalid calls produce a diagnostic incident; failures remain in the run record. Document actual invocation commands once implemented.

Deliver M1 and stop. Report changes, commands run, limitations, and the next milestone. Do not implement M2 unless requested.

## M2: Real provider and controlled tool execution

Status: complete on the local macOS ARM64 host. ChatGPT account sign-in, direct inference, controlled edits, independent checks, and restart inspection are verified. Fishnet implemented its own inspection summary feature. See [M2 evidence](m2-evidence.md).

Select one direct API provider. Preserve native content needed across turns, stream and parse tool calls, record usage and capability limits, and add bounded process execution with explicit roots and policy checks. Mock the network in default tests. Add prompt-section fingerprints and provider cache fields.

Acceptance: opt-in live smoke test works with externally supplied credentials; malformed responses and interrupted streams have defined outcomes; final effective arguments are checked before execution; uncertain side effects are not blindly retried. Measure harness time separately from provider, tool, and user wait.

## M3: Service and remote-client contract

Status: complete on the local macOS ARM64 host. Durable acceptance, authenticated commands, idempotency, approval ownership, bounded consumers, reconnect snapshots, and restart inspection pass offline and real loopback checks. See [M3 evidence](m3-evidence.md).

Expose the existing core through a long-running daemon. Move the CLI onto the same command/event contract. Add authentication, command idempotency, reconnect cursors, snapshot recovery, and bounded consumers. Record API/transport decisions. Add a small headless client fixture; a web UI is not required.

Acceptance: a client disconnect does not cancel accepted work; reconnect returns correct state; duplicate submissions do not repeat work; unauthorized reads and commands fail; a slow client cannot cause unbounded memory growth. Multiple clients cannot resolve one approval twice.

## M4: TUI

Status: complete on the local macOS ARM64 host. The terminal client shares M3 task state and supports approvals, cancellation, detach/reconnect, bounded output, and explicit outcomes. The usability revision adds one-command startup, guided Rust tasks, readable CLI output, responsive layouts, and command acknowledgement at exit. Offline screen/input cases and real pseudo-terminal checks passed. See [M4 evidence](m4-evidence.md) for the remaining qualification limits.

Build the TUI as a client of M3. Render task state, streamed output, approvals, cancellation, and bounded diagnostic summaries. Keep business logic in the service.

Acceptance: CLI and TUI observe the same session; attach/detach and cancellation preserve state; test terminal rendering and input independently of live model calls.

## M5: Plugins, skills, and middleware

Implement package validation and MCP transports selected in an ADR. Add portable and Codex fixtures, then defined Claude component mappings. Implement ordered typed hooks and observers. Compare extension runtime candidates before choosing one. Include a reference plugin and compatibility report.

Acceptance: path escape and unsupported required components are rejected clearly; mutations precede authorization; handler failure behavior is explicit; cancellation and timeouts are measured; reload does not silently change active policy versions. Test middleware that calls downstream then fails, ensuring no duplicated side effects.

## M6: Workspace knowledge

Implement explicit multiple roots/repository identities and instruction scopes. Add text retrieval and evidence-linked wiki data. Separate memory storage, retrieval, and context engine interfaces. Evaluate graph-assisted retrieval before selecting a graph database.

Acceptance: conflicting instructions resolve by documented scope; source edits/deletions invalidate derived knowledge; cross-repository changes retain starting revisions; facts and inferences stay distinguishable; graph evaluation includes a non-graph baseline.

## M7: Policy experiments

Implement local incident selection, candidate changes, repeatable fresh trials, holdout cases, comparison reports, and policy version rollback. Keep automatic promotion disabled initially.

Acceptance: demonstrate one bounded policy change that reduces unnecessary work while preserving success; include failures and uncertainty; evaluation checks are protected; replay cannot re-execute external writes; every accepted result identifies its inputs and versions.

## Status updates

After each requested milestone, record completion evidence here or link a status document. Do not mark a stage complete from scaffolding or passing tests unrelated to its acceptance criteria.
