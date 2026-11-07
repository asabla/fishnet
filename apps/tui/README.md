# Terminal client

Start the service in one terminal:

```sh
make daemon
```

Open the TUI in another terminal:

```sh
make tui
```

Press `n` to start an offline fixture. The view shows task state, operation
output, independent checks, counts, and terminal outcomes. It updates from the
same service that `fishnet task` uses. It does not run a separate model loop.

| Key | Action |
| --- | --- |
| Up/Down or j/k | Select a task |
| n | Submit an offline success fixture |
| p | Open the coding task form |
| a | Review a pending approval |
| c, then y | Cancel the selected task |
| PageUp/PageDown | Scroll recent output |
| Left/Right | Scroll output horizontally |
| End | Follow latest output |
| v | Open recent status and uncertain command identities |
| r | Refresh connection and task list |
| ? | Show help |
| q or Ctrl-C | Detach; accepted service work continues |

In a form, Tab changes fields and Enter submits. Esc closes a dialog. Approval
review uses y to approve, n to reject, and arrows/PageUp/PageDown to scroll the
complete grant. The service accepts one decision at the displayed revision.

For coding, stop the default daemon and start it explicitly with:

```sh
make daemon ARGS="--allow-code"
```

Prepare a reviewed host task under `tasks/`, then press p, enter its path and
optional model/account, and submit. Press a to review the frozen grant before
y approves it. The default host provider is the ChatGPT account connected with
`fishnet auth login`. API-key profiles are explicit daemon options; see
[service usage](../../services/daemon/README.md). No credential is entered into
the TUI. Default startup, rendering tests, and fixtures need no account.

Other connection and attach options:

```sh
make tui ARGS="--connection PATH --attach TASK_ID"
make tui ARGS="--task tasks/YOUR_TASK.json --model MODEL"
make tui ARGS="--help"
```

`--task` opens a prefilled form; it does not approve execution. The client needs
an interactive terminal of at least 60 columns and 15 rows. It reports an
unavailable daemon and retries reads. It reloads the connection file after a
daemon restart. Neither reconnect nor detach repeats a command.

The output pane keeps bounded previews of completed durable events. It does not
show transient token deltas or replace the full journal. A retention gap is
explicit and recovers from the service snapshot. Use `fishnet task status` or
local `fishnet inspect` for detailed records. Interrupted runs remain unknown;
model completion alone remains unverified until host checks pass.
