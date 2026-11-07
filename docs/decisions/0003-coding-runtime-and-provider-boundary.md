# ADR-0003: Controlled coding tasks and direct model adapters

Status: accepted

Date: 2026-11-07

## Problem and constraints

Fishnet must become useful for its own development. It must own execution,
retain independent checks, and support provider adapters without delegating the
loop to another agent runtime. M3 services and M4 terminal UI are not needed for
this first local workflow.

## Decision

Retain two workspace packages. Extend the core with a typed `ModelProvider`
boundary, a workspace policy, and bounded process ownership. Use pinned Tokio
1.53.2 for interruptible network and process I/O. Use Reqwest 0.12.28 with Rustls
for direct HTTP requests. Use SHA-256 through sha2 0.10.9 for source and prompt
fingerprints. Keep journal writes and file work off the asynchronous I/O driver.

The first wire adapter uses OpenAI Responses. Authentication is separate from
this adapter. ChatGPT OAuth and API-key access use explicit, separate CLI routes.
An explicit Responses-compatible endpoint can use a provider-specific credential
variable. This does not establish compatibility with every provider. Other wire
protocols require their own adapters and fixtures.

The host task file fixes the objective, root, writable files, required changes,
protected check sources, named argv commands, and checks. The model can select a
command ID. It cannot supply replacement argv or change the in-memory grant.
Program names resolve before the run is accepted. The effective argv is recorded
before execution. There is no general shell-string tool.

File tools reject traversal, internal credential/state paths, and symlinks.
Edits require the current complete-file hash and one exact old-text match. They
prepare and sync a sibling file, check current content again, then rename it.
A failed durability boundary has an unknown-effect error and stops the run.
There is no automatic retry. Unix processes have an owned process group, bounded
capture, an explicit cwd, no stdin, and a restricted environment. Cancellation
and deadlines terminate and reap the owned process before returning.

Native Responses output remains in history and records, including opaque
reasoning items and assistant phase. Only completed responses can dispatch a
function call. Malformed, interrupted, multiple, or reused call IDs fail before
execution. Requests disable parallel tool calls and automatic HTTP retry.
Provider-reported usage remains raw and normalized. Missing usage remains null.
Prompt-section hashes describe local content; they do not establish cache hits.

Independent checks run after the model finishes. A zero status also requires the
contract's source changes and unchanged protected check sources. Check output
can be explicitly truncated while its completion status remains recorded. A
failed check remains a failure. No check gives unverified completion.

## Evidence and validation

Read the official [Responses streaming contract](https://developers.openai.com/api/docs/guides/streaming-responses),
[function calling contract](https://developers.openai.com/api/docs/guides/function-calling),
and [reasoning-item contract](https://developers.openai.com/api/docs/guides/reasoning).
Read versioned Tokio and Reqwest documentation and cached source before use.
Tests use in-memory SSE fixtures, disposable files, and fixed local test commands.
See [M2 evidence](../m2-evidence.md) for current qualification.

## Consequences and compatibility

Schema 2 adds native provider observations, effective commands, and workspace
snapshots. Inspection still accepts schema 1. Neither version resumes or reruns
operations. Schema 2 phase durations exclude the start-journal write; schema 1
had different timing semantics. Old records do not receive a fabricated harness
duration.

Process policy is an application capability boundary. It is not an OS sandbox.
Approved programs and project code run with the user's OS permissions. Use this
workflow with trusted projects and reviewed command grants. Cancellation can
leave completed edits or partial command effects. Concurrent hostile filesystem
mutation is outside this milestone's protection.

## Reversal or migration path

Add adapters behind the same typed boundary. Keep provider-native content and
capability limits explicit. Add an OS execution sandbox when its threat model is
selected. Add a service only when M3 is requested. Do not add policy promotion or
automatic memory generation to this workflow.
