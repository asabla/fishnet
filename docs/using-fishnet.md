# Developing fishnet with fishnet

Use the local service and its terminal client for new interactive work. The
Rust service owns accepted tasks, execution policy, checks, and records. The
TUI can detach and reconnect without stopping the task.

## Connect the host account

```sh
make cli ARGS="auth login"
make cli ARGS="auth status"
make cli ARGS="models"
```

Complete the printed sign-in URL in your browser. Review the account and plan
permission. Tokens stay in `.fishnet/auth`. Do not paste them into chat or
commit them. Account access and direct coding were qualified in M2; see
[M2 evidence](m2-evidence.md). The service still determines current model access.

## Start an interactive task

In the first terminal:

```sh
make daemon ARGS="--allow-code"
```

In the second terminal:

```sh
make tui
```

Use `make daemon` without `--allow-code` for offline fixtures only. Press n in
the TUI for a fixture. For coding, create a reviewed JSON contract under `tasks/`
using [task authoring](../tasks/README.md). Press p, enter its path and optional
model/account, then submit. Press a to inspect the frozen provider, root, file
grants, commands, limits, and independent checks. y approves; n rejects.

The completed `tasks/bootstrap-summary.json` remains a qualification fixture.
Its feature is already implemented. Do not rerun it unchanged; it requires a
new source change. Write a new contract for the next task.

The host repeats the selected checks after the model finishes. A model final
message cannot establish success. Review the Git diff before committing.
Cancellation can leave completed edits or partial command effects. Fishnet does
not roll them back or automatically retry an uncertain operation. Approved
programs run with the user's OS permissions; use trusted projects.

## Keep the running harness fixed during self-development

Build and copy the service and client before a task rebuilds their source:

```sh
make build
mkdir -p .fishnet/bin
cp target/debug/fishnetd .fishnet/bin/fishnetd
cp target/debug/fishnet-tui .fishnet/bin/fishnet-tui
```

Start `.fishnet/bin/fishnetd --allow-code` and `.fishnet/bin/fishnet-tui` in
separate terminals. These paths assume the default Cargo target directory.
Restart the service only when you intend to load a new harness version. Existing
unfinished work remains unknown after a crash; restart never replays it.

## Inspect and reconnect

```sh
make cli ARGS="task list"
make cli ARGS="task status TASK_ID"
make cli ARGS="task watch TASK_ID --after SEQUENCE"
make tui ARGS="--attach TASK_ID"
```

q or Ctrl-C in the TUI detaches. Use c then y for explicit cancellation. The CLI
`task cancel TASK_ID` uses the same service. Inspection and reconnect perform
reads, not execution. The local `fishnet inspect TASK_ID --summary` command can
read full journal evidence even when the daemon is stopped. See
[TUI usage](../apps/tui/README.md) for keys and limits.

## Other provider routes

Configure a service API-key profile explicitly on the host:

```sh
make daemon ARGS="--allow-code --provider openai --key-env OPENAI_API_KEY --model MODEL"
make daemon ARGS="--allow-code --provider responses --endpoint https://PROVIDER/v1/responses --key-env PROVIDER_API_KEY --model MODEL"
```

These are separate alternatives. Do not start two owners of the same service
store. The second route requires a Responses-compatible API; it is not universal
provider compatibility. Native adapters can be added behind `ModelProvider`.
ChatGPT account mode does not silently switch to API-key billing.

Use one service host per working tree. That service admits one coding task at a
time and keeps further approvals pending. Separate hosts and standalone commands
do not share the execution lock. Do not use them to edit the same tree concurrently.

Standalone M1 `run` and M2 `code` commands remain available for compatibility.
They do not gain daemon ownership or reconnect guarantees. Live coding through
the new TUI was not run during qualification; see [M4 evidence](m4-evidence.md)
for the independently checked terminal and service behavior.
