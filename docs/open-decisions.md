# Open decisions

Resolve a decision when the current milestone needs it. Avoid speculative dependency selection.

| Decision | Options or questions | Required evidence |
| --- | --- | --- |
| Rust toolchain and crate boundaries | M1: Rust 1.94.1, core and CLI; [ADR-0001](decisions/0001-rust-workspace-and-quality.md) | Offline local checks passed; revisit boundaries when needed |
| Async/runtime stack | M1: synchronous Rust owner with interruptible fixture waits; async choice deferred | M2 provider/process requirements will guide the async decision |
| Persistence | M1: bounded JSONL journals; [ADR-0002](decisions/0002-run-journal-and-fixture-evidence.md) | Process restart tests passed; concurrent storage and migrations remain future work |
| Remote interface | HTTP/SSE, WebSocket, ACP adapter | Reconnection, approvals, terminal streams, SDK generation |
| First model provider | Direct API adapter selected for available credentials | Streaming/tool-call fixture coverage and capability limits |
| Middleware runtime | TypeScript/Python worker, Lua, Wasm | Authoring experience, cancellation, isolation, overhead |
| Compatibility scope | Portable/Codex/Claude packages; optional Hermes runtime adapter | Real fixtures and behavioral mappings |
| Execution isolation | Host process, OS sandbox, container, remote worker | Explicit capabilities and target platform coverage |
| Knowledge backend | SQL/text baseline, embeddings, graph store | Retrieval quality, source freshness, latency and cost |
| Promotion authority | Reports first; later bounded automatic promotion | Reliable checks, rollback, regression evidence |
| Supported operating systems | Initial host macOS; other targets to confirm | Honest platform-specific tests and limitations |
| License and distribution | Undecided private project | Owner decision before public distribution |

Decision records use [the template](decisions/template.md). Candidate libraries in research are not approvals or locked dependencies.
