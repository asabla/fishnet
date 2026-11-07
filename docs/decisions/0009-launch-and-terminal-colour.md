# ADR-0009: Launch aliases and terminal colour policy

Status: accepted

Date: 2026-11-07

## Problem

The user reported that test runs did not resemble the renderer previews.
Inspection found two concrete differences. `fishnet run` still invoked the M1
JSON fixture utility. Also, the validation PTYs inherited `NO_COLOR=1`, while
TestBackend previews retained RGB styles. The terminal encoder suppressed the
colours and backgrounds. Earlier PTY checks had verified text and lifecycle,
but had not asserted the emitted colour sequences.

## Decision

Make `fishnet run` an interactive alias, matching `fishnet`, `fishnet tui`, and
`make run`. Rename the standalone JSON utility to `fishnet fixture`; retain
`make fixture` as its developer entry point. This is an intentional CLI change.
The fixture contracts, stored events, outcomes, and inspection semantics stay
the same. Existing journals require no conversion or replay.

Support `--color auto|always|never` at TUI entry points. Auto respects nonempty
NO_COLOR. Always explicitly enables the existing dark theme. Never uses the
terminal's own colours with reverse-video surfaces. Label monochrome mode and
show the override in the footer. Apply the selection through the pinned
Crossterm 0.29 `force_color_output` API; do not mutate process environment values.
No dependency is added.

## Evidence and limits

A default test renders actual cells through Crossterm into bytes. It invokes
four isolated child processes to test auto with/without NO_COLOR, always with
NO_COLOR, and never. It checks the RGB accent sequence or reverse-video panels
and rejects empty combined colour sequences. Process isolation avoids changing
Crossterm's global colour cache across parallel tests.

Real PTYs separately check installed, checkout, run-alias, and Make launch paths.
These checks test command routing and terminal encoding. They do not establish
native terminal appearance or every emulator's colour capability. See
[M4 evidence](../m4-evidence.md) for results and the corrected qualification limit.
