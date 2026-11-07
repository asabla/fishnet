# Local task service

Start the daemon in one terminal:

```sh
make daemon
```

It binds to an automatic loopback port and saves `.fishnet/service/connection.json`
with private permissions. It prints the endpoint and connection-file path, never
the credential. Keep the file private. Start with `--allow-code` only when you
want reviewed coding tasks to be available:

```sh
make daemon ARGS="--allow-code"
```

Use another terminal for the common CLI client:

```sh
make cli ARGS="task list"
make cli ARGS="task submit --request-id example --scenario success"
make cli ARGS="task watch TASK_ID"
make cli ARGS="task status TASK_ID"
make cli ARGS="task cancel TASK_ID"
```

Submission returns acceptance, not a successful task result. `watch` waits for a
terminal snapshot and returns the task outcome exit code. Ctrl-C detaches the
watch; it does not cancel accepted work. Cancellation is a separate command.

For coding, create a reviewed host task under `tasks/`, then submit and inspect
its grant. The API freezes that task and host provider settings before approval:

```sh
make cli ARGS="task submit --request-id change-1 --task tasks/YOUR_TASK.json"
make cli ARGS="task status TASK_ID"
make cli ARGS="task approve TASK_ID --revision 1"
make cli ARGS="task watch TASK_ID"
```

Use `task reject TASK_ID --revision 1` to deny it. One decision can consume the
revision. The service defaults to the host's ChatGPT account. API-key profiles
are host configuration, never credentials supplied in task requests. See
`make daemon ARGS="--help"` for `--provider`, `--endpoint`, `--key-env`, and
`--model`. Do not put secrets in arguments or task files.

Use `--connection PATH` on client commands for a different service store. Use
`task watch TASK_ID --after SEQUENCE` to reconnect from a known event cursor.
An explicit gap reports the authoritative snapshot and the retained cursor.
Unknown work after a crash remains unknown; reconnect never executes it again.

The version 1 API is defined in `src/contract.rs`: list/submit at `/v1/tasks`,
snapshots at `/v1/tasks/{id}`, event pages at `/v1/tasks/{id}/events`, and explicit
`approval` and `cancel` operations below that path. Requests and replies use
`schema_version` and `data` envelopes. Authentication applies to every route.
Event pages contain at most 8 events and retain a live window of 128. The full
journal is still inspectable locally. `after` must not exceed the known cursor;
`wait_ms` is at most 15000. All writes are bounded and idempotency receipts are
retained. A full store rejects new work instead of forgetting accepted IDs.

The daemon serves one operator on loopback. Use a protected tunnel for remote
access. Browser origins, public HTTP listeners, multi-user roles, and automatic
restart of task execution are not enabled. Ctrl-C stops admission, cancels active
work, and waits for owned cleanup. The service retains unresolved approvals.

One coding task can execute at a time within a service. Further approvals remain
pending until that task finishes or is cancelled. Read-only fixtures can use the
other execution slots. Use one service host per working tree. Separate service
stores and standalone commands do not share this execution lock.

## Normal interactive startup

`fishnet` now starts or reuses this working tree's local service. It uses a
separate service process, enables coding preparation, and retains per-task grant
approval. Startup makes no provider request. `fishnet stop` uses the authenticated
`POST /v1/shutdown` command. Explicit `--connection` clients never start a host.
Upgrade service and client together. See [ADR-0007](../../docs/decisions/0007-interactive-entry-and-guided-rust-tasks.md).

The guided `rust` submission supplies an objective and exact paths. The host
freezes a profile with fmt/test/Clippy/docs commands; clients do not supply argv.
Preparation does not contact a provider. Known provider initialization failures
before core execution have phase `failed`; uncertain interrupted execution has
phase `interrupted`. Both remain persisted, distinct from verified completion.

Task commands now show readable output. Add `--json` when consuming the examples
above from scripts. The standalone utility behavior remains unchanged.
