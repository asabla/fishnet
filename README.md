# fishnet

A planned Rust agent harness with CLI and TUI clients, remote-client support, portable plugins, event middleware, and local execution diagnostics.

This initial structure contains documentation only. It has no application code, Cargo manifests, dependency choices, or executable tests.

## Start here

1. Read [AGENTS.md](AGENTS.md).
2. Read [requirements](docs/requirements.md) and the [architecture proposal](docs/architecture.md).
3. Follow the [implementation plan](docs/implementation.md).
4. Use the [Codex handoff](docs/codex-handoff.md) to start implementation.

## Layout

| Path | Purpose |
| --- | --- |
| `apps/cli/` | Scriptable command client |
| `apps/tui/` | Interactive terminal client |
| `services/daemon/` | Long-running service and remote API |
| `crates/` | Rust libraries; create actual crates when needed |
| `tests/fixtures/` | Sanitized protocol, recovery, and compatibility fixtures |
| `evals/cases/` | Task evaluation cases and outcome checks |
| `plugins/examples/` | Reference plugins when the extension contract exists |
| `docs/decisions/` | Architecture decisions and their evidence |

The directory layout expresses responsibilities, not a requirement to create one crate per directory. Implement a small vertical slice first.
