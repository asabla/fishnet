# fishnet

fishnet is an independent Rust agent harness. M1 runs a scripted model, one
read-only fixture tool, and an independent completion check. The Rust core owns
the loop, execution policy, task state, cancellation, and local diagnostics.

## Run M1

Use the pinned Rust toolchain. On a new machine, `cargo fetch --locked` populates
the dependency cache. Application runs and default tests need no network access
or provider credentials.

```sh
cargo run --offline --locked -p fishnet-cli -- run
```

The command emits JSON events and saves them under `.fishnet/runs/`. Read
`task_id` from an event. Inspect that run from a new process:

```sh
cargo run --offline --locked -p fishnet-cli -- inspect YOUR_RUN_ID
```

Inspection never executes tools. An unfinished record stays incomplete. A model
final message alone produces `unverified_completion`. Only the independent
fixture check can produce `verified_success`.

```sh
cargo run --offline --locked -p fishnet-cli -- run --scenario repeated-invalid
cargo run --offline --locked -p fishnet-cli -- run --scenario check-failure
cargo run --offline --locked -p fishnet-cli -- run --scenario negative
cargo run --offline --locked -p fishnet-cli -- run --step-delay-ms 10000 --deadline-ms 30000
```

Press Ctrl-C during the delayed run to request cancellation. See
[CLI usage](apps/cli/README.md) for limits, scenarios, and exit codes.

## Check the workspace

```sh
./scripts/check.sh
```

This runs formatting, offline tests, strict Clippy, and rustdoc with the pinned
toolchain. See the [development policy](docs/development.md) for enforced rules
and manual code review requirements.

## Status and boundaries

M1 is complete. See [acceptance evidence](docs/m1-evidence.md),
[implementation status](docs/implementation.md), and
[technology decisions](docs/decisions/README.md).

There is no real model provider, shell execution tool, daemon, TUI, plugin host,
web UI, memory generation, or policy promotion in M1. The next milestone is M2:
a direct provider and controlled tool execution. No M2 implementation is included.

Read [AGENTS.md](AGENTS.md), [requirements](docs/requirements.md), and
[architecture](docs/architecture.md) before making changes.
