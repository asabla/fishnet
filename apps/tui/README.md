# Terminal interface

Run `fishnet` in the project directory. It opens the interface and starts a
private loopback service if this working tree has no running service. From the
checkout, `make run` does the same. A second terminal is not required.

The local service prepares coding tasks, but no account request or model request
occurs at startup. Each coding task needs approval of its complete grant.

## Start a task

Press `n`. Describe the result, then name the exact files to change, separated
by commas. Leave the files field blank to prohibit source edits. Press Enter to
prepare the grant. The Rust service selects fixed Cargo formatting, test,
Clippy, and documentation commands. It records the grant before returning it.

Review the provider, model, workspace, file access, commands, checks, and limits.
Use arrows or PageUp/PageDown to read the full grant. Press `y` to approve or `n`
to reject. Preparation alone does not contact the provider. Files listed for
change must differ when checks run. This guided profile requires Cargo.toml.

Quality checks establish the listed checks. They do not prove every requirement
in an objective. Review the diff and use a protected behavioral check when the
change needs one. Press `p` for an advanced reviewed task file under `tasks/`.
See [task authoring](../../tasks/README.md).

## Keys

| Key | Action |
| --- | --- |
| n | Describe a new Rust task |
| f | Run an offline fixture without an account |
| p | Open an advanced task file |
| Tab | Show or hide tasks; wide windows can show a sidebar |
| Up/Down or j/k | Select a task |
| Enter | Close task selection, or review a pending grant |
| a | Review the selected pending grant |
| c, then y | Cancel the selected task |
| PageUp/PageDown | Scroll wrapped activity |
| End | Follow latest activity |
| v | Open identities, timings, diagnostics, and recent status |
| r | Refresh connection and task list |
| ? | Open scrollable help |
| q | Detach after outstanding commands are acknowledged |
| Ctrl-C | Detach immediately; print identities of uncertain commands |

Forms support Tab/Shift-Tab, cursor movement, Home/End, Backspace/Delete, Ctrl-U,
and bounded paste. Enter continues to review; a pasted newline never submits.
Esc closes a dialog. Help, details, and grant review use arrows, PageUp/PageDown,
Home, and End. Scroll limits are recalculated after resize.

## Accounts and connections

Connect a ChatGPT account outside the TUI with `fishnet auth login`. Credentials
stay in the host's private account store. No credential is entered in a form.
For other providers, configure the host explicitly with `fishnet serve`; see
[service usage](../../services/daemon/README.md).

```sh
fishnet --model MODEL --account LABEL
fishnet attach TASK_ID
fishnet --task tasks/YOUR_TASK.json
fishnet tui --connection PATH
fishnet tui --no-start
fishnet stop
```

An explicit connection only attaches; it never starts or replaces a host.
A task file opens a prefilled form and does not approve execution. The automatic
host remains alive after detachment. `fishnet stop` requests authenticated
shutdown, cancels active work, and retains pending approvals. Use one service
host per working tree. Independent hosts and standalone writers do not share
its coding execution lock.

## Display and evidence limits

The interface requires 40 columns and 12 rows. Small windows use one content
pane. Long output wraps. Modal views clear their background so old borders and
text do not overlap their controls. Labels distinguish verified, unverified,
failed, cancelled, and unknown outcomes without relying on color.

The activity view keeps 256 recent lines with 512-byte previews; explicit
markers report truncation. Full journals remain on disk. Completed durable
events are displayed; transient token deltas are not implemented. Reconnect
never repeats a command. A lost response remains uncertain until inspection.
A failure before core execution is labelled failed; an uncertain interrupted
execution remains unknown.

See [M4 evidence](../../docs/m4-evidence.md) for exact qualification boundaries.
