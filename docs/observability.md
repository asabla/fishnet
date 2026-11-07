# Local diagnostics

Telemetry stays local by default. Export is optional and explicit. The agent can query bounded summaries; it must not need raw full-session records in every prompt.

## Common fields

Use schema version; session/task/turn/operation/attempt identifiers; parent/cause IDs; event sequence; timestamp; duration; policy, plugin, tool and provider versions; workspace/revision references; outcome; evidence reference. Clock time supports correlation; monotonic time measures durations within a process. Do not infer a global event order from independent host clocks.

## Measurements

| Area | Measures |
| --- | --- |
| Outcome | Verified success, unverified completion, failure, cancellation, correction, rework |
| Model | Queue/request/first-token/total duration, attempts, tokens, stop reason |
| Tools | Logical calls, validation errors, permission waits, execution time, output size |
| Context | Selected source IDs, fingerprint sections, token budget, compaction/retrieval time |
| Cache | Reads/writes, local hits/misses, invalidations, reported divergence, inferred cause |
| Runtime | Queue depths, resource usage, dropped observer events, storage latency |
| Recovery | Intervention trigger, action, cost, verified progress, repeat count |

Distinguish parent wall time from overlapping child duration. Preserve raw provider usage and normalize with explicit semantics. Missing fields are unknown. Cache misses do not directly prove misunderstanding. Exclude cold/ineligible requests from a claimed reusable-cache hit rate, or clearly state the denominator when eligibility is unavailable.

Fingerprint normalized operation arguments with relevant state: repository/revision/content, tool version, execution root, and configuration. Repeated reads after a change are not equivalent. Fingerprints over sensitive data need appropriate protection; hashes alone are not anonymization.

Separate expected negative evidence from schema, transport, environment, permission, and execution failures. Never infer verified success from exit status alone.

Redact credentials before persistence. Bound payload sizes and retention. Keep large outputs as artifact references. Distinguish never collected, redacted, truncated, expired, and unavailable evidence. Preserve enough aggregate records to include failed and cancelled runs in evaluation statistics.
