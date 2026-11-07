# Instructions for Codex

## Goal

Build fishnet, an independent Rust agent harness. The canonical repository location is `/Users/asabla/code/private/fishnet`. The harness must own its model/tool loop, session state, execution policy, and local diagnostics.

## Read before implementation

Read `docs/requirements.md`, `docs/architecture.md`, `docs/improvement-loop.md`, `docs/implementation.md`, and `docs/open-decisions.md`. Read the other contracts when their component enters scope.

Treat requirements as requirements and proposed technologies as candidates. The user has not chosen a middleware language, database, web framework, model provider, or graph backend. Record decisions with reasons and evidence in `docs/decisions/`. Choose reversible details autonomously; ask only when missing information materially changes behavior, authorization, or scope.

## Working method

- Inspect the repository and applicable instructions before changing it. Preserve user changes and existing Git history.
- Implement one milestone at a time. When scope is unspecified, implement the earliest incomplete milestone in `docs/implementation.md` and report its result.
- Keep the runtime and production execution loop in Rust. Do not use Codex, Claude Code, or another agent runtime as the hidden primary engine. An optional external-agent adapter is a separate future feature.
- Keep clients thin. Business logic, authorization, diagnostics, and task state belong to the core service interfaces.
- Use explicit typed state and versioned boundary contracts. Do not serialize internal calls unnecessarily.
- Use bounded queues, cancellation, deadlines, and explicit ownership. Do not block async workers with database or CPU-heavy work.
- Use the smallest dependency set and crate structure that serves the milestone. Do not create empty abstractions for every possible provider.
- Verify current primary documentation before adopting changing external protocols. Pin versions and retain test fixtures. See `docs/research.md` for starting references, not guaranteed specifications.
- Do not claim compatibility from manifest parsing alone. Report support per component and tested version.
- Do not claim that a task succeeded from the model's final message alone. Record independent checks or mark the result unverified.
- Do not automatically turn transcripts into durable memories or skills. Improve policies through incidents, experiments, and versioned releases.
- Retain failures, cancellations, and unknown outcomes in evaluation statistics. Never optimize tool-call count by skipping verification or hiding operations in a batch.
- Do not add a graph database, autonomous policy promotion, hosted telemetry dependency, plugin marketplace, or bespoke web UI before a milestone needs it.

## Scope of this initial package

The initial repository is documentation and placeholders only. That is its current state, not a permanent prohibition on code. An implementation request authorizes code for the requested milestone. Do not implement all future milestones merely because they are documented.

## Validation and delivery

During Rust implementation, use `cargo fmt --check`, relevant unit/integration tests, and `cargo clippy --workspace --all-targets -- -D warnings` once the workspace exists. Add meaningful tests for state transitions, cancellation, persistence, authorization, and external contracts. Use mocked providers by default; live requests must be explicitly enabled and must not require committed secrets.

Test crash recovery at side-effect boundaries. Never retry an uncertain external write merely because its response was lost. Keep fixture replay distinct from live strategy evaluation.

Update status and decisions as work progresses. Report changes, checks actually run, remaining limitations, and the next milestone. Do not invent benchmark numbers or time estimates. Do not commit or push unless requested. Do not install project content as a global agent skill.

Use direct, formal language. Follow ASD-STE100 principles for technical explanations: short sentences, consistent terms, and explicit actions. Do not use emojis.
