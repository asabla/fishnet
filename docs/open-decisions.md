# Open decisions

Resolve a decision when the current milestone needs it. Avoid speculative dependency selection.

| Decision | Options or questions | Required evidence |
| --- | --- | --- |
| Rust toolchain and crate boundaries | Stable toolchain; small workspace | Build and maintenance needs of the first slice |
| Async/runtime stack | Tokio or justified alternative | Cancellation, process, streaming, and portability support |
| Persistence | SQLite or another embedded store | Crash consistency, migrations, contention, backup and retention |
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
