# M2 acceptance evidence

Status: complete on the local development host, 2026-11-07. M3 is not implemented.
The user authorized continued development through the first useful self-development
workflow, then selected ChatGPT account access and approved source transfer for
the protected task. These requests extended the original M1-only scope.

## Delivered behavior

The Rust core owns the asynchronous model/tool loop, task state, authorization,
process lifecycle, durable events, and independent checks. The CLI adds account
sign-in, model discovery, explicit coding task contracts, and compact inspection.
The production loop does not call another agent runtime.

Direct Responses requests preserve native output, opaque reasoning items, phase,
and call IDs across turns. The adapter parses bounded SSE events and waits for
`response.completed`. Interrupted, incomplete, failed, malformed, multiple, or
reused calls cannot dispatch a tool. Automatic HTTP retries are disabled.

The observed live HTTP responses omitted Content-Type. The adapter permits an
absent header, records `sse-untyped`, and applies the same SSE framing, byte,
JSON, completion, and tool-call checks. A present unsupported media type fails.
There is no JSON-response fallback. Raw usage, normalized optional usage fields,
request timing, and prompt-section fingerprints remain in local records.

The host fixes the root, writable files, required changes, protected check
sources, named argv commands, and independent checks. Edits require a current
file hash and a unique old-text match. Effective argv is recorded before process
execution. Processes have bounded output, cancellation, deadlines, and owned
Unix process groups. An uncertain write or missing process result stops the run.
Inspection reconstructs records without constructing a provider or executing tools.

## Protected self-development task

Fishnet used the connected ChatGPT account and `gpt-6.1-sol` to implement
`inspect --summary` in `apps/cli/src/inspection.rs`. No other file had a model
write grant. The checker source and task policy were protected. The independent
checker failed before the feature existed. Its final run passed against the
rebuilt CLI, preserved full inspection, and confirmed unchanged journal bytes.

The harness ran from a copied executable while the task rebuilt the CLI:

```sh
rustc --edition 2024 --test tests/bootstrap/inspect_summary.rs -o .fishnet/bootstrap/inspect-summary-check
cargo build --offline --locked -p fishnet-cli
cp target/debug/fishnet .fishnet/bin/fishnet-m2
.fishnet/bin/fishnet-m2 code --task tasks/bootstrap-summary.json --model gpt-6.1-sol
```

Successful run: `1794072311705214000-93372-0`. The terminal evidence references
operation `1794072311705214000-93372-0-op-45`. All six host checks passed after
the model's final response: build, protected acceptance, formatting, workspace
tests, Clippy, and documentation. Each check also confirmed required source
changes and unchanged protected files. The model's final message was not the
success criterion. The resulting feature is isolated in commit `281fd20`.

| Recorded measurement | Value |
| --- | ---: |
| Provider attempts | 20 |
| Logical tool calls | 19 |
| Process attempts, including host checks | 12 |
| Rejected tool calls | 1 |
| Total duration, microseconds | 97,236,489 |
| Provider phase, microseconds | 83,261,812 |
| Tool phase, microseconds | 9,174,903 |
| Independent check phase, microseconds | 4,212,889 |
| Harness residual, microseconds | 586,885 |
| Reported input tokens, summed over requests | 153,045 |
| Reported output tokens, summed over requests | 1,278 |
| Reported cached input tokens, summed over requests | 25,728 |

All 20 successful provider turns supplied usage. These are observations from one
trial, not a benchmark or a cache-hit-rate claim. The single rejected call read
an unavailable path; it did not execute a process. Process counts include the
model's six command selections and the host's six independent repetitions.

Schema 2 operation durations exclude the durable start write. Harness time is
the task duration minus sequential provider, tool, and check durations. It
includes storage, observation delivery, and other work outside those phases;
it is not a CPU-time measure. Account sign-in and user consent occurred before
task acceptance. There was no user-input wait inside this task.

| Reproducibility input | SHA-256 |
| --- | --- |
| Frozen harness executable, local only | `331dda2d7c63af219332eed6cece1352d9aaa42c671439acf746fdfc30ec01c3` |
| Initial inspection source | `894d43bfa802457989c3ca52cb095c04efca66fb9be7e8c149ba9fa765f84590` |
| Final inspection source | `0f64aed609a93b760900e3b2898c851e5f672af4dac5a12b28e4596cdfa5eee8` |
| Protected acceptance source | `52f4fa5c3a01db2e92635bdaabc506b4d8c39d1f2aee1d2fc37400405b7a3e2f` |

The journal also contains the task contract, policy version, starting file
hashes, effective programs and arguments, response IDs, native output, operation
IDs, and checks. It is private local evidence under the ignored `.fishnet/runs`.
It is not a project memory, skill, or policy release.

## Retained unsuccessful trials

All six live trials remain in the local store. None was removed from the
qualification result. Later trials followed a specific observed correction;
there was no automatic retry of an uncertain side effect.

| Run ID | Outcome and observation |
| --- | --- |
| `1794071695297592000-89524-0` | Failed before tools: missing SSE content type |
| `1794071792602647000-90242-0` | Failed before tools: adding JSON handling did not resolve the response |
| `1794071863654369000-90461-0` | Failed before tools: diagnostics identified an absent content-type header |
| `1794071906704410000-90641-0` | Failed at 32 calls: conservative file pages consumed the budget during required reading |
| `1794072059789923000-91282-0` | Failed at 32 calls after source edits and commands: no final model turn remained |

The speculative JSON fallback was removed. File pages now use the complete
serialized result size, including escaping, to enforce the same output cap.
The request reports the remaining model-turn budget and disables tool selection
on the last turn. It does not skip the host's checks. The earlier generated
source was preserved locally before restoring only the task-owned initial file
for the final fresh trial. A lower call count was not used as proof of success.

## Checks actually run

Host: macOS 27.2, ARM64. Compiler: rustc 1.94.1. Cargo: 1.94.1. Direct
dependencies and the complete lockfile are pinned. The initial dependency cache
was populated before offline validation.

`./scripts/check.sh` passed with the final code:

```sh
cargo fmt --all -- --check
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo doc --workspace --no-deps --offline --locked
```

There are 51 passing tests: 4 existing CLI cases, 1 protected CLI acceptance
case, 12 core unit cases, 4 contract cases, 17 M1 cases, and 13 M2 cases. Default
tests make no provider requests and need no credentials. Cases cover native
content/result correlation, malformed and interrupted streams, signed OAuth
claims, credential-file permissions and locks, path grants, stale edits,
protected checks, file-page escaping, cancellation around edits, process-tree
termination, output limits, deadlines, environment filtering, and restart records after an applied edit loses its result.

The runtime, account access, and CLI commits each passed the complete check
script in an isolated source snapshot. The final code and tests at `fe7e9eb` also passed
the script in a source export with no `.fishnet` directory or account files,
using the populated Cargo cache. No hosted CI result is claimed.

A new CLI process inspected the successful run and an existing schema 1 M1
record. The journal hash did not change during inspection. Schema 1 reports an
unknown harness duration because its timing semantics differ from schema 2.

## Limits and next milestone

- Live qualification covers this ChatGPT account, this model, and this host.
  API-key OpenAI and explicit Responses-compatible routes have offline fixtures;
  they were not independently qualified against other live services. Native
  protocols for other providers require separate adapters.
- Approved commands run with the user's OS permissions. This is application
  policy for trusted projects, not an OS sandbox. Credentials are omitted from
  the inherited command environment. Same-user hostile processes and concurrent
  hostile filesystem mutation are outside this boundary.
- Credential files use private Unix permissions and are not encrypted at rest.
  Refresh and remote revocation have fixture coverage but were not live-tested.
- Files and native context have explicit bounds. There is no context compaction,
  automatic recovery of unfinished effects, or automatic rollback of edits.
- Journals have per-record bounds. Store-wide retention is manual. Blocking CLI
  output and filesystem synchronization can delay cooperative cancellation.
- Linux, Windows, hosted CI, daemon clients, TUI, plugins, knowledge backends,
  automatic memories, and policy promotion are not qualified by this work.

The next planned milestone is M3: the service and remote-client contract.
Further local development can use reviewed task files through the current CLI;
M3 does not need to be implemented to use this starting workflow.
