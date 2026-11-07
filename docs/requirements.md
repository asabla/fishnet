# Requirements

Status: recorded user requirements and derived constraints. M1 is implemented; see [acceptance evidence](m1-evidence.md). Later product capabilities remain pending.

## Fixed requirements

- Project name: fishnet. Canonical path: `/Users/asabla/code/private/fishnet`.
- Rust owns the harness and its model/tool loop. Performance must be measured rather than inferred from the language.
- Provide CLI and TUI clients. Provide an API that lets a web client connect to a running instance. A custom web UI is not required.
- Support portable plugin packages with manifests, skills, resources, and local or remote MCP servers. Assess current portable Agent Plugins and Codex layouts; include Claude compatibility in the research and fixture plan.
- Support application extensions at defined operation boundaries, including before, after, failure, cancellation, and observation behavior. The extension language is open.
- Support workspaces containing one repository, several repositories, or ordinary directories and documents.
- Support evidence-linked knowledge, wiki content, and graph-assisted retrieval through replaceable interfaces.
- Collect diagnostics locally: task outcome, cache behavior, tool errors, retries, call counts, duration, context use, and resource use.
- Let the harness inspect bounded diagnostic summaries. A central collection service must not be required.
- Improve execution policy through measurable incidents and controlled experiments. A session observation is not automatically a durable memory or reusable instruction.

## Derived constraints

- Verify task intent and outcomes before comparing efficiency.
- Separate observed facts, inferred causes, and accepted policy changes.
- Keep the same execution semantics across clients and transports.
- Preserve cancellation and restart behavior. Client disconnect does not imply task cancellation.
- Keep repository identity, file access, instruction scope, revision, and memory scope separate.
- State compatibility limits and unknown outcomes explicitly.
- Keep raw provider usage as well as normalized usage. Unsupported or missing metrics are unknown, not zero.

## Not decided

No middleware language, database, provider, transport bundle, graph engine, frontend framework, deployment platform, or public license has been selected. See [open decisions](open-decisions.md).
