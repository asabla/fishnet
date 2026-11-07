# fishnet

fishnet is an independent Rust agent harness. M1 provides offline fixtures. M2
adds direct model access, controlled repository edits, approved commands, and
independent coding checks. The Rust core owns
the loop, execution policy, task state, cancellation, and local diagnostics.

## Run M1

Use the pinned Rust toolchain. On a new machine, `cargo fetch --locked` populates
the dependency cache. Fixture runs and default tests need no network access
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

M1 and M2 are complete on the local macOS ARM64 host. Fishnet used the connected
ChatGPT account to implement its own `inspect --summary` feature. The host's six
independent checks passed. See [M2 evidence](docs/m2-evidence.md),
[M1 evidence](docs/m1-evidence.md), [implementation status](docs/implementation.md),
and [technology decisions](docs/decisions/README.md).

Use [the development workflow](docs/using-fishnet.md) to connect a ChatGPT account
and prepare a bounded coding task. API-key OpenAI and explicit Responses-compatible
routes remain separate. Other native provider protocols need adapters and tests.

Approved programs run with your OS permissions. Use reviewed task grants and
trusted projects. There is no OS sandbox, daemon, TUI, plugin host, knowledge
backend, memory generation, or policy promotion. The next planned milestone is
M3: the service and remote-client contract.

Read [AGENTS.md](AGENTS.md), [requirements](docs/requirements.md), and
[architecture](docs/architecture.md) before making changes.
