# Developing fishnet with fishnet

Install the command from the checkout, then open it in the project directory:

```sh
make install
fishnet
```

For checkout development, `make run` opens the same interface. The command starts
or attaches to the local Rust service. No second terminal is required. Startup
makes no model request. The service owns accepted work after the interface exits.

## Connect the host account

```sh
fishnet auth login
fishnet auth status
fishnet models
```

Complete the printed sign-in URL in your browser. Review the account and plan
permission. Tokens stay in `.fishnet/auth`. Do not paste or commit them. Status
reads local data; model listing makes an account request. Account output is
readable by default; use `--json` for scripts.

## Prepare and review a task

Type the required result in the prompt. Enter opens file access selection.
Name exact files to change, separated by commas. Enter then prepares a Rust task
with fixed Cargo formatting, tests, Clippy, and documentation checks. It does not start a provider. Review the complete
file grant, resolved commands, provider, account, model, checks, and limits.
Press y to approve or n to reject.

This profile requires Cargo.toml. It permits at most 32 exact file paths and
requires each named file to change. Blank files prohibit source edits. Commands
can still write build artifacts and have the user's OS permissions. Use trusted
projects. The profile protects existing root AGENTS.md, rust-toolchain.toml,
and scripts/check.sh. For different grants or protected behavioral checks, use
/taskfile and a reviewed contract under `tasks/`. See [task authoring](../tasks/README.md).

The host repeats the selected checks after model completion. A final model
message cannot establish success. Quality checks prove their listed criteria;
they do not prove every claim about the requested behavior. Review the source
diff before committing. Cancellation can leave completed edits or partial
command effects. Fishnet does not roll back or blindly retry uncertain work.

Use /fixture for an offline example and /help for help. Each submission is a
separate task; earlier messages are not sent as context for a new task. See [TUI usage](../apps/tui/README.md).

## Inspect, detach, and stop

```sh
fishnet tasks
fishnet status TASK_PREFIX
fishnet watch TASK_PREFIX
fishnet attach TASK_PREFIX
fishnet cancel TASK_PREFIX
fishnet stop
```

Task commands accept a unique ID prefix. They show readable output by default;
use `--json` to retain complete machine-readable records. /quit waits for outstanding
command acknowledgements, then detaches. Ctrl-C detaches immediately and reports
pending or uncertain command identities. Accepted work continues. `fishnet stop`
requests service shutdown and cancellation of active work. Pending approvals
remain available after restart.

Inspection and reconnect perform reads, not execution. The offline command
`fishnet inspect TASK_ID --summary` reads a full journal without a running
service. No operation is repeated to reconstruct state. Use one service host per
working tree; separate hosts and standalone commands do not share the execution
lock.

## Keep the running harness fixed during self-development

Build and copy the command before a task rebuilds its source:

```sh
make build
mkdir -p .fishnet/bin
cp target/debug/fishnet .fishnet/bin/fishnet
.fishnet/bin/fishnet
```

These paths assume the default Cargo target directory. The same frozen command
starts its service mode. Stop an existing service before changing its version.
A disconnected client does not replace a running host. Unfinished work after a
crash remains unknown; restart never replays it.

## Advanced host configuration

The automatic host uses the current working tree and its default ChatGPT account.
For an explicit host or another provider, run one of these alternatives:

```sh
fishnet serve --allow-code
fishnet serve --allow-code --provider openai --key-env OPENAI_API_KEY --model MODEL
fishnet serve --allow-code --provider responses --endpoint https://PROVIDER/v1/responses --key-env PROVIDER_API_KEY --model MODEL
```

Then run `fishnet`, or connect with `fishnet tui --connection PATH` for a custom
store. Explicit connections never start a host. Use `fishnet serve --help` for
root, store, account-directory, and bind options. Service logs from automatic
startup are in `.fishnet/service/daemon.log`, with one bounded previous log.

Responses compatibility is not universal provider compatibility. Native adapters
can be added behind ModelProvider. ChatGPT mode does not silently switch to
API-key billing. Standalone M1 `run` and M2 `code` remain diagnostic/compatibility
utilities; they do not gain daemon ownership. The completed bootstrap-summary
contract is retained as evidence and must not be rerun unchanged.

See [M2 evidence](m2-evidence.md) and [M4 evidence](m4-evidence.md) for checks and
limits. Qualification of a local workflow does not establish support for every
terminal, platform, project, or provider.
