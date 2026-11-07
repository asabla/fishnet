# CLI

The CLI calls the Rust core locally. It parses arguments, delivers Ctrl-C, and
writes JSON. Execution policy, state, storage, and checks remain in the core.

```sh
cargo run --offline --locked -p fishnet-cli -- --help
cargo run --offline --locked -p fishnet-cli -- run --scenario success --store .fishnet/runs
cargo run --offline --locked -p fishnet-cli -- inspect YOUR_RUN_ID --store .fishnet/runs
```

`run` writes one versioned JSON event per line. Each event is persisted before it
is published. `inspect` writes one JSON run record, including events, statistics,
terminal outcome, pending operations, and partial-tail status. It does not need
a provider or reconstruct state by executing tools.

| Scenario | Expected outcome |
| --- | --- |
| `success` (default) | Verified success after reading and checking the greeting |
| `unverified` | Final model message without tool evidence; unverified completion |
| `check-failure` | Tool result contradicts the protected contract; failed |
| `repeated-invalid` | Two equivalent rejected requests; incident and unverified completion |
| `negative` | Expected missing key; verified success, no transport error |
| `provider-failure` | Scripted provider error; failed |

| Option | Default | Supported range |
| --- | --- | --- |
| `--store` | `.fishnet/runs` | Explicit local directory |
| `--deadline-ms` | 5000 | 0..=300000; zero expires before execution |
| `--step-delay-ms` | 0 | 0..=300000; fixture wait for cancellation/deadline checks |
| `--max-output-bytes` | 4096 | 32..=8192, serialized model/tool response bytes |
| `--max-provider-calls` | 16 | 1..=32 |

These execution options apply to `run`. `inspect` accepts only its run ID and
`--store`. IDs cannot contain paths. A closed stdout pipe does not stop journal
completion. A blocked output write can delay cooperative cancellation.

| Exit code | Meaning |
| --- | --- |
| 0 | Verified success, successful inspection, or help |
| 2 | Task failure, invalid input, storage error, or output error |
| 3 | Unverified completion |
| 130 | Cancellation |

The inspected outcome can be failed or cancelled even when inspection exits 0.
A process killed without a terminal record remains incomplete, with an unknown
outcome. Inspection reports that state and does not retry it.
