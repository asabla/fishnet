# Terminal interface

Run `fishnet` or `fishnet run` in the project directory, or `make run` from this checkout. The
command opens the interface and starts or reuses its private local service.
Startup makes no model request. The service owns accepted work after detach.

## Terminal colours

Use `fishnet --color always` to select the colour theme explicitly. From this
checkout, use `make run ARGS="--color always"`. `--color auto` is the default;
it respects a nonempty `NO_COLOR` variable. In that case, input and tool blocks
use reverse video so their boundaries remain visible. `--color never` selects
that monochrome presentation explicitly. No environment variable is changed.

The colour previews show the colour theme. Monochrome mode is labelled in the
footer. Actual terminal escape bytes are checked separately from renderer cells.
`fishnet run` opens this interface. `fishnet fixture` prints offline JSON events.

## Send a message

Type a message and press Enter. The first message opens a compact provider
consent dialog. Enter or `y` allows it; `n` rejects it. F2 shows full details.
Later messages in that conversation share the same consent. Chat has no file or
command access. It sends messages only, including earlier replies in that
conversation. Startup and preparation make no provider request.

Each turn has a durable task record linked to its parent. Attach to a saved turn
with `fishnet attach TASK_ID`, then send another message. Restart inspection
reads journals; it does not repeat model or tool calls. History is session data,
not user or project memory. Responses remain unverified.

The context limit is 16 turns and 32 KiB of message text. Messages are limited
to 4096 bytes; replies to 8192 bytes. A full context or a truncated reply requires
`/new`. No earlier context is silently discarded. A failed or cancelled turn
cannot pass on consent. Changing provider, account, or requested model requires
a new conversation. Only completed turns can receive a later message.

## Prepare a code task

Use `/code` before entering a code task. Enter then selects exact writable files,
separated by commas. Leave the field blank to prohibit source edits. Each named
file must change. Esc returns to the retained prompt. The guided profile requires
Cargo.toml and selects Cargo formatting, tests, Clippy, and documentation checks.
Code tasks remain separate and do not inherit chat context or consent.

The compact grant names the provider, model, workspace, file access, and checks.
Use F2 to inspect command arguments, protected files, limits, and fingerprint.
The service enforces the displayed grant revision. Quality checks do not prove
every requirement in an objective. Review the diff and protect behavioral checks
when needed. `/taskfile` opens a reviewed contract under `tasks/`. See
[task authoring](../../tasks/README.md).

To test conversation without an account, start an isolated host with
`fishnet serve --provider fixture --allow-code`. Then connect with
`fishnet --connection PATH`. This provider returns labelled, deterministic text.
It never calls a live provider and cannot run code tasks. The separate `/fixture`
command exercises the existing model/tool/result/model fixture.

## Commands and keys

Type `/` to see commands. Type a prefix and press Tab to complete a unique match.
Enter executes a command. Paste alone never executes one.

| Command | Action |
| --- | --- |
| /new, /chat | Start a separate conversation |
| /code | Switch to explicit code task preparation |
| /tasks | Open saved tasks; arrows select, Enter opens, Esc closes |
| /review | Review pending file and command access |
| /cancel | Open cancellation confirmation; `y` confirms |
| /details | Show identities, diagnostics, timings, and recent event output |
| /fixture | Run the offline example without an account |
| /taskfile | Open an advanced task contract |
| /refresh | Refresh the connection and task list |
| /help | Show keys and limits |
| /quit | Detach after outstanding commands are acknowledged |

| Key | Action |
| --- | --- |
| Enter | Send a message, prepare a code task, or execute a command |
| Alt-Enter, Shift-Enter, or Ctrl-J | Insert a new line; terminal support varies, so Ctrl-J is the fallback |
| Left/Right, Home/End | Move the prompt cursor |
| Backspace/Delete, Ctrl-U | Remove text or clear the prompt |
| Ctrl-P | Open saved tasks |
| PageUp/PageDown | Read earlier or later transcript content |
| Esc | Follow the latest content or close a panel |
| F1 / F2 | Open help / details |
| Ctrl-C | Detach immediately; print pending or uncertain command identities |

The prompt keeps unsent text when selecting another task. Startup shows a new
prompt; it does not silently open a previous task. Use `fishnet attach TASK_ID`
to attach directly. Esc does not cancel a task. Cancellation can leave edits and
partial command effects; it does not roll them back.

## Accounts and connections

Connect a ChatGPT account outside the TUI with `fishnet auth login`. Credentials
stay in the host's private store. For other providers, configure the host with
`fishnet serve`; see [service usage](../../services/daemon/README.md).

```sh
fishnet --model MODEL --account LABEL
fishnet attach TASK_ID
fishnet --task tasks/YOUR_TASK.json
fishnet tui --connection PATH
fishnet tui --no-start
fishnet stop
```

A service from an earlier binary must be restarted before chat works. Finish active work, run `fishnet stop`, then reopen fishnet. Pending grants and journals remain. The client checks `conversation-v1` before sending a chat request. It never retries a submission to an incompatible host.

An explicit connection only attaches; it never starts or replaces a host.
A task file opens a prefilled form and does not approve execution. The host
remains alive after detach. `fishnet stop` requests shutdown and cancellation of
active work. Pending approvals remain after restart. Use one service host per
working tree. Separate hosts and standalone writers do not share its execution
lock. Commands use host OS permissions; there is no OS sandbox.

## Display and evidence limits

The interface requires 40 columns and 12 rows. The start screen centres the
prompt below the fishnet wordmark. Active tasks use a single transcript column
with a maximum width of 100 cells and a prompt at the bottom. The model row
shows the configured request preference, or "host default" when none is set.
It does not guess the selected provider. Tool results use compact shaded blocks;
independent checks and authoritative outcomes stay explicit. Task selection,
file access, grant review, and diagnostics use temporary panels. Explicit state
labels distinguish verified, unverified, failed, cancelled, and unknown outcomes.
Model text cannot establish success. Routine connection messages stay in
details. Errors and uncertain command outcomes remain visible across reconnect.

The view retains 128 transcript entries with 4096-byte, 48-line previews, plus
256 recent diagnostic lines with 512-byte previews and 16 status entries. The
prompt is limited to 4096 bytes; file fields to 512 bytes. Truncation is marked.
Full journals remain on disk. Chat check bookkeeping stays in details; the footer retains its unverified outcome. Only completed durable events are displayed.
Reconnect never repeats operations. Replayed event sequences are not appended
twice. A lost command response remains uncertain until inspection.

For manual colour review, export actual renderer cells with:

```sh
FISHNET_LAYOUT_PREVIEW_DIR="$PWD/.fishnet/layout-preview" \
  cargo test -p fishnet-tui --test layout --offline --locked
```

Each retained row snapshot exports HTML under the same name. Exports do not
share filenames across tests. The HTML is test evidence, not an application
web client. It does not
prove rendering in a native terminal. See [M4 evidence](../../docs/m4-evidence.md)
for the checks actually run and the remaining qualification limits.
