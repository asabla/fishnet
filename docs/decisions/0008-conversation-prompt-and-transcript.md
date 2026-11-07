# ADR-0008: Conversation prompt and durable transcript projection

Status: accepted

Date: 2026-11-07

## Problem and constraints

The user rejected the task dashboard after trying M4. Letter shortcuts prevented
normal typing. A modal task form and a raw event feed made the interface hard to
use. Passing layout tests had not established a suitable interaction model.

The user requested inspection of actual Codex and OpenCode TUI screenshots.
The [Codex fullscreen announcement](https://github.com/openai/codex/discussions/49129)
and [OpenCode terminal page](https://dev.opencode.ai/terminal) were inspected in
Browser. Their visible common structure is a main conversation, a persistent
prompt, compact tool activity, and small secondary controls. These are visual
references, not claims of compatibility or local execution of either product.

## Decision

Keep Ratatui, Crossterm, and the current crate boundaries. Add no dependencies.
Replace global letter shortcuts with ordinary text entry and slash commands.
Centre the start prompt below the fishnet wordmark. Keep the active-task prompt
at the bottom. Grow it for multiline text, with a height limit.
Keep task selection, file access, grant review, and diagnostics in temporary
panels. Use a single transcript column, spacing, subtle backgrounds, and explicit
outcome labels. Limit the start column to 80 cells and the active column to 100.
Use a dark surface and a narrow blue edge on the composer, with a request-model
row and a small project/shortcut row. Unknown model selection is labelled as
host default. Retain a plain wordmark at narrow widths. Preserve the full host grant and its revision before approval.

Project durable events into bounded typed transcript entries. Group tool and
check results with their operation identifiers. Keep provider requests and other
protocol details in diagnostics. Keep the projection separate from the journal;
it cannot change execution or determine an outcome. Ignore replayed sequences
and clear the projection when task identity changes. Inspection never reruns a
tool. Preserve unsent text across selection and late acknowledgements. Classify client notices as information, connection
status, or attention. Keep routine connection status in details and retain
attention notices and uncertain-command counts across reconnect.

The host still accepts independent tasks. Enter first selects exact writable
files and then prepares the frozen grant. It does not silently grant repository
writes or send previous task text as model context. Each named file must change.
The client cannot implement persistent conversational turns without a matching
service and runtime contract. That work, transient text streaming, rich Markdown,
and inline diff presentation remain within the M4 usability revision.

## Evidence

See the conversation revision in [M4 evidence](../m4-evidence.md). Input and
renderer tests are offline. Optional HTML export preserves actual TestBackend
cell symbols and styles for manual Browser review. Export by snapshot name so
parallel tests cannot overwrite another state's preview. It is a test artifact, not a
new product web UI. PTY behavior, renderer previews, native terminal appearance,
and live provider evidence are reported separately.

## Consequences and reversal

The letter shortcut map in ADR-0006 and ADR-0007 is replaced. /quit waits for
command acknowledgement; Ctrl-C detaches immediately. /cancel still requires
confirmation. Restore the previous view by changing the client only; execution,
authorization, journals, and the versioned service contract remain unchanged.

This increment fixes the main input and layout. It does not establish parity
with Codex or OpenCode, or complete the conversational product workflow.
