# fishnet

An independent Rust agent harness. Fishnet owns its model/tool loop, task state,
execution policy, checks, and local records.

## Start

Use the pinned Rust toolchain. Populate the dependency cache once, then install:

```sh
make setup
make install
fishnet
```

Run fishnet in the project directory. `fishnet run` opens the same interface. It opens the terminal interface and starts
a private local service if needed. No second terminal is required. From this
checkout, `make run` opens the same interface. `make` lists development commands.

Type a Rust task in the prompt. Enter selects exact files to change.
The service prepares formatting, test, Clippy, and documentation checks. Review the full
provider and file grant before approving it. Startup and preparation do not
make model requests. Use `/fixture` for an offline example, `/help` for help,
and `/quit` to detach. If `NO_COLOR` is set, use `fishnet --color always` to
select the colour theme shown in the previews. `fishnet fixture` is the offline
JSON utility.

Connect your account with `fishnet auth login`. Credentials stay private. See
[the development workflow](docs/using-fishnet.md) and [TUI usage](apps/tui/README.md).

## Commands

```sh
fishnet tasks
fishnet status TASK_PREFIX
fishnet watch TASK_PREFIX
fishnet attach TASK_PREFIX
fishnet cancel TASK_PREFIX
fishnet stop
```

Task IDs accept unique prefixes. Task and account output is readable by default;
use `--json` for scripts. `fishnet --help` lists normal and advanced commands.
Accepted tasks remain service-owned when the TUI detaches. Stop cancels active
work. Pending approvals and run records remain inspectable after restart.

## Development and evidence

```sh
make check
make fixture
```

Default tests and fixtures need no network or credentials. The quality command
checks formatting, workspace tests, strict Clippy, and rustdoc. See
[development policy](docs/development.md) for enforced checks and Code Intent
review. [Technology decisions](docs/decisions/README.md) record selected details.

M1 through M4 are implemented on the local macOS ARM64 host. The original M4
checks established technical behavior but did not establish a usable normal
workflow. The subsequent usability revision covers the single-command entry,
guided task preparation, responsive layouts, and command acknowledgement at
exit. See [M4 evidence](docs/m4-evidence.md) for current checks and limits.
An approved disposable Rust task also completed through the installed TUI and
passed all four independent host checks.
Fishnet's earlier ChatGPT self-development task is recorded in [M2 evidence](docs/m2-evidence.md).

Approved commands use host OS permissions. There is no OS sandbox, plugin host,
knowledge backend, memory generation, or automatic policy promotion. Generic
Rust quality checks do not prove every behavioral requirement. Review diffs and
add protected behavior checks when needed. M5 remains the next planned milestone.

Read [AGENTS.md](AGENTS.md), [requirements](docs/requirements.md), and
[architecture](docs/architecture.md) before implementation work.
