# Terminal interface

Run `fishnet` in the project directory, or `make run` from this checkout. The
command opens the interface and starts or reuses its private local service.
Startup makes no model request. The service owns accepted work after detach.

## Write a task

Type a Rust task in the bottom prompt. Ordinary letters enter text. Press Enter
to select exact files the task can change, separated by commas. Leave that field
blank to prohibit source edits. Enter then prepares the complete host grant.
Esc returns to the retained prompt. No provider call starts during preparation.

Review the provider, model, workspace, file access, commands, checks, and limits.
Use arrows or PageUp/PageDown to read the grant. Press `y` to approve or `n` to
reject. The service records and checks the displayed approval revision.

The guided profile requires Cargo.toml and selects Cargo formatting, tests,
Clippy, and documentation checks. Each named file must change. Quality checks
establish their listed criteria; they do not prove every requirement in an
objective. Review the diff and protect behavioral checks when needed. Use
`/taskfile` for a reviewed contract under `tasks/`. See [task authoring](../../tasks/README.md).

Each submission is a separate task. The current service does not send earlier
conversation text as context for a new submission. This revision corrects the
prompt and transcript interaction; persistent conversational turns and transient
token display remain unfinished M4 work.

## Commands and keys

Type `/` to see commands. Type a prefix and press Tab to complete a unique match.
Enter executes a command. Paste alone never executes one.

| Command | Action |
| --- | --- |
| /new | Clear the selected view and start a new task |
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
| Enter | Prepare the prompt or execute a slash command |
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

An explicit connection only attaches; it never starts or replaces a host.
A task file opens a prefilled form and does not approve execution. The host
remains alive after detach. `fishnet stop` requests shutdown and cancellation of
active work. Pending approvals remain after restart. Use one service host per
working tree. Separate hosts and standalone writers do not share its execution
lock. Commands use host OS permissions; there is no OS sandbox.

## Display and evidence limits

The interface requires 40 columns and 12 rows. It shows a single transcript,
compact tool results, independent checks, and a persistent prompt. Task selection,
file access, grant review, and diagnostics use temporary panels. Explicit state
labels distinguish verified, unverified, failed, cancelled, and unknown outcomes.
Model text cannot establish success.

The view retains 128 transcript entries with 4096-byte, 48-line previews, plus
256 recent diagnostic lines with 512-byte previews and 16 status entries. The
prompt is limited to 4096 bytes; file fields to 512 bytes. Truncation is marked.
Full journals remain on disk. Only completed durable events are displayed.
Reconnect never repeats operations. Replayed event sequences are not appended
twice. A lost command response remains uncertain until inspection.

For manual colour review, export actual renderer cells with:

```sh
FISHNET_LAYOUT_PREVIEW_DIR="$PWD/.fishnet/layout-preview" \
  cargo test -p fishnet-tui --test layout --offline --locked
```

The exported HTML is test evidence, not an application web client. It does not
prove rendering in a native terminal. See [M4 evidence](../../docs/m4-evidence.md)
for the checks actually run and the remaining qualification limits.
