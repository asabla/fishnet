# Original M1 implementation handoff

This is the original task text, retained as historical context. M1 and M2 are
complete. Use [current status](implementation.md) and the user's current request
to select work; do not treat this archived task as a new instruction.

Copy the following task into Codex with `/Users/asabla/code/private/fishnet` open as its workspace. Ensure the documentation skeleton is in that directory first. Preserve its existing `.git` directory. This task authorizes M1 implementation, not all future stages.

---

Implement the first working milestone of fishnet in `/Users/asabla/code/private/fishnet`.

Read AGENTS.md and the linked design documents before editing. Inspect existing files and Git status. Preserve existing work. Follow `docs/implementation.md`, milestone M1. Complete M1 end to end; do not stop at a plan, and do not continue into M2.

fishnet is an independent Rust agent harness. It must own its model/tool loop, state, execution policy, and local diagnostics. The full product will support CLI, TUI, remote web clients through an API, portable plugins and MCP, typed middleware, multiple repository workspaces, and replaceable context/knowledge interfaces. These are future capabilities, not a reason to build all components now.

For M1, create a minimal Rust workspace with a CLI, core state machine, scripted fixture provider, and deterministic read-only tool. Execute a model-tool-result-model cycle without network access or secrets. Add structured events, operation correlation, cancellation, bounded output, durable inspection after restart, and independent fixture outcome checks. Distinguish verified success, unverified completion, failure, and cancellation.

Implement initial diagnostics for equivalent repeated invalid tool calls against unchanged relevant state. Do not classify an expected failing test or useful negative search result as transport failure or automatic waste. Store observations and incidents separately from user/project memory. Do not generate memories or skills from sessions.

Keep dependencies and crate boundaries small. Candidate technologies in the research are not fixed decisions. Make necessary reversible choices and record them as decision records. Do not use an existing agent runtime as the hidden execution engine. Do not add a graph database, custom web UI, external middleware runtime, shell tool, real provider integration, or autonomous policy promotion in M1.

Validate cancellation and recovery. Inspection after restart must not rerun tools. Record uncertain effects rather than retrying them blindly. Keep failure and cancellation records in statistics. Tool-call count is a diagnostic signal, not the success objective.

Run formatting, relevant tests, and Clippy once the Cargo workspace exists. Use deterministic fixtures in default tests. Update the implementation status with evidence. Report commands actually run, remaining limitations, and the next milestone. Do not commit or push.

Use direct, formal technical English, short sentences, and consistent terms. No emojis or invented time estimates.
