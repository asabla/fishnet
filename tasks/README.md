# Authoring a coding task

`bootstrap-summary.json` is the completed first self-development task. Keep it
as a qualification fixture. It requires a source change and is not an unchanged
post-installation smoke test.

For the next task, create a separate JSON file from that example. Review these
fields before enabling a provider:

- `schema_version`: use 1 for task files.
- `objective`: state the required behavior, constraints, and completion evidence.
- `workspace.root`: choose one trusted local root. Relative roots use the CLI's
  working directory. `--root` can select it explicitly.
- `allowed_writes`: list exact relative files. This is not a directory grant.
- `required_changes`: list granted files that must differ from their starting
  hashes. Use an empty list for a deliberate read-only verification task.
- `protected_paths`: name existing check sources, instructions, and policy files
  that must not change. The task file protects itself when it is under the root.
- `commands`: select fixed program/argument vectors, unique IDs, and timeouts.
  A single program name resolves through PATH before execution. A relative
  program path resolves under the root. Review executable and script provenance.
- `checks`: select command IDs that the host must run independently after the
  model finishes. Include checks for the requested behavior and relevant quality
  checks. Protect the behavioral checker from candidate edits.
- `limits`: choose a deadline up to 300000 milliseconds, 1 through 32 model turns,
  and 2048 through 8192 serialized output bytes. The last model turn disables
  tool selection. The host still performs every required check.

A zero exit code does not replace required source-change evidence or protected
check integrity. Without independent checks, completion stays unverified.
Nonzero command output can be useful evidence during development. It does not
cause an automatic retry or a transport-error classification.

Run a frozen copy of fishnet when a task rebuilds its own CLI. Capture its JSON
output in `.fishnet/`, inspect the terminal record, and review the source diff.
Commit reviewed changes separately. Fishnet does not automatically commit or
promote policy changes. Preserve failed and cancelled records when comparing
trials. A task file is an explicit provider-access and command decision; it is
not a security sandbox for untrusted build scripts or tests.
