# Developing fishnet with fishnet

The starting workflow is a local CLI with an explicit task contract. A daemon,
TUI, plugin runtime, or external agent engine is not required.

## Connect an account

```sh
cargo run --offline --locked -p fishnet-cli -- auth login
cargo run --offline --locked -p fishnet-cli -- auth status
cargo run --offline --locked -p fishnet-cli -- models
```

Open the printed sign-in URL in your browser. Review the fishnet account and plan
permissions. Tokens stay in `.fishnet/auth` and must not be pasted into chat or
committed. Use `--account LABEL` to keep another registration separate. Use
`auth logout` to stop using the selected account and request session revocation.

The service controls application eligibility and model access. A successful
login does not establish that an inference request will succeed. ChatGPT mode
uses the public API and never falls back to API-key billing.

## Prepare a bounded task

The JSON task selects exactly which files can change and which commands can run.
The completed qualification task, `tasks/bootstrap-summary.json`, grants changes
to `apps/cli/src/inspection.rs` only. The acceptance source and task contract remain
protected. Keep the task scope small and review the command list.

The bootstrap task is retained as acceptance evidence. Its feature is already
implemented. Do not rerun it unchanged: its contract requires a new source change.
For a new task, create a reviewed contract as described in
[task authoring](../tasks/README.md).

The original qualification prepared its independent checker before starting
the model:

```sh
mkdir -p .fishnet/bootstrap .fishnet/bin
rustc --edition 2024 --test tests/bootstrap/inspect_summary.rs -o .fishnet/bootstrap/inspect-summary-check
cargo build --offline --locked -p fishnet-cli
cp target/debug/fishnet .fishnet/bin/fishnet-m2
```

The copied harness stays fixed while a task rebuilds the CLI under development.
The independent checker exercises the rebuilt CLI and checks the requested
behavior. It does not accept the model's final claim as evidence.

```sh
.fishnet/bin/fishnet-m2 code --task tasks/YOUR_TASK.json
```

Use `--model NAME` to select a listed account model. Without it, fishnet uses the
first visible model in the account catalog. Run records retain the actual model,
provider output, usage, operation IDs, durations, edits, and check results.
Inspect the emitted task ID from another process. Inspection does not resume work.

```sh
fishnet inspect RUN_ID --summary
```

Account access and a live coding task passed on the development host. See
[acceptance evidence](m2-evidence.md) for the model, task, failed trials, and limits.

## Other provider routes

API-key access is explicit:

```sh
fishnet code --task TASK.json --provider openai --model MODEL --key-env OPENAI_API_KEY
fishnet code --task TASK.json --provider responses --model MODEL --endpoint https://PROVIDER/v1/responses --key-env PROVIDER_API_KEY
```

The second route requires a Responses-compatible API. It is not a general claim
of compatibility with every model service. Native adapters can be added behind
`ModelProvider`. Provider-native behavior must have its own tests and live checks.
ChatGPT credentials cannot be routed through these options automatically.

## Review the result

A verified outcome requires the host's checks and required source changes.
Review the Git diff before committing. Failure, cancellation, unverified
completion, and interrupted operations remain inspectable. Commands can have
partial effects when interrupted. Fishnet does not roll them back or retry an
uncertain operation. Use trusted projects; approved commands run with your OS
permissions. This application policy does not provide an OS sandbox.
