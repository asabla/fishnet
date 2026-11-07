# Open decisions

Resolve a decision when the current milestone needs it. Avoid speculative dependency selection.

| Decision | Options or questions | Required evidence |
| --- | --- | --- |
| Rust toolchain and crate boundaries | M1: Rust 1.94.1, core and CLI; [ADR-0001](decisions/0001-rust-workspace-and-quality.md) | Offline local checks passed; revisit boundaries when needed |
| Async/runtime stack | M2: Tokio and Reqwest; [ADR-0003](decisions/0003-coding-runtime-and-provider-boundary.md) | Offline stream/process fixtures and a protected live coding task passed |
| Persistence | M1: bounded JSONL journals; [ADR-0002](decisions/0002-run-journal-and-fixture-evidence.md) | Process restart tests passed; concurrent storage and migrations remain future work |
| Remote interface | M3: authenticated loopback HTTP with versioned JSON and bounded event polling; [ADR-0005](decisions/0005-local-service-and-client-contract.md) | Offline contract cases and real CLI reconnect/restart passed; public hosting and browser clients remain open |
| First model provider | M2: direct Responses, ChatGPT account access; [ADR-0004](decisions/0004-chatgpt-account-access.md) | ChatGPT live task passed; other live services and native protocols remain unqualified |
| Middleware runtime | TypeScript/Python worker, Lua, Wasm | Authoring experience, cancellation, isolation, overhead |
| Compatibility scope | Portable/Codex/Claude packages; optional Hermes runtime adapter | Real fixtures and behavioral mappings |
| Execution isolation | M2: explicit host argv grants and Unix process groups; OS sandbox undecided | Local macOS tests passed; trusted-project boundary only |
| Knowledge backend | SQL/text baseline, embeddings, graph store | Retrieval quality, source freshness, latency and cost |
| Promotion authority | Reports first; later bounded automatic promotion | Reliable checks, rollback, regression evidence |
| Supported operating systems | Initial host macOS; other targets to confirm | Honest platform-specific tests and limitations |
| License and distribution | Undecided private project | Owner decision before public distribution |

Decision records use [the template](decisions/template.md). Candidate libraries in research are not approvals or locked dependencies.
