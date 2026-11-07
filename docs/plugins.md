# Plugin and middleware contract requirements

Status: contract requirements; schema and runtime selection are pending.

Separate package/content portability from runtime behavior. A package may contain skills, resources, MCP endpoints, middleware, and client UI contributions. Each component needs a support status. Do not assume a hosted vendor app mapping grants access outside that vendor.

Validate manifests, version requirements, relative resource paths, symlink escapes, executable declarations, and requested capabilities. Pin installed content and provenance. Resolve secrets outside package files. Do not run repository-local plugin code merely because it was discovered.

## Operation boundaries

Before hooks may propose validated input changes, deny, or request approval. Authorization evaluates final effective inputs. After hooks may transform the presented output, but retain original evidence. Error hooks can classify failures; the core owns retry decisions. Finally behavior must cover cancellation. Observer queues are bounded and their drop/backpressure policy is explicit.

Execution wrappers may invoke downstream at most once or short-circuit with a typed result. A handler failure after downstream ran must never trigger downstream again. Detect recursion and impose deadlines. Cancellation of an async task does not prove that a subprocess or external service stopped; track the resulting uncertainty.

Use deterministic order and explicit dependencies; reject cycles. Define per-class failure policy. A required authorization check blocks on failure. An optional observer can fail without blocking execution. Capabilities and isolation are separate: a process boundary alone is not a sandbox.

Keep event versions and compatibility mappings explicit. Hermes turn hooks and provider-request hooks have different timing. Claude hooks and mods expose different contracts. No adapter may silently reinterpret an approval decision.

## Compatibility evidence

For each fixture record source/version, component types, expected behavior, unsupported features, and result. Start with synthetic or permissively licensed sanitized fixtures. Do not claim all plugins from an ecosystem work.
