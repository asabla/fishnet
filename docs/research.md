# Research starting points

Reviewed in the design conversation on 2026-11-07. These links are references, not accepted implementation specifications. Recheck live documentation and pin exact source revisions before implementing compatibility. Earlier research was a documentation review, not a benchmark or full source audit.

| Source | What to investigate |
| --- | --- |
| [Agent Plugins](https://agent-plugins.org/) | Portable manifests, skills, MCP, extension namespaces |
| [OpenAI plugin packaging](https://developers.openai.com/plugins/build/plugins) | Portable layout and Codex compatibility |
| [Codex app-server client](https://github.com/openai/codex/blob/main/codex-rs/app-server-client/README.md) | Typed core and external transport boundaries |
| [OpenCode server](https://opencode.ai/docs/server/) | Shared service for multiple clients |
| [Pi extensions](https://pi.dev/docs/latest/extensions) | Ordered transformations, cancellation, extension contracts |
| [OpenHands architecture](https://docs.openhands.dev/sdk/arch/overview) | Separation of agent, tools, workspace, and server |
| [Goose architecture direction](https://goose-docs.ai/blog/2026/04/08/goose-acp-and-new-tui/) | Migration toward shared client protocol; check current status |
| [Hermes architecture](https://hermes-agent.nousresearch.com/docs/developer-guide/architecture) | Runtime and subsystem boundaries |
| [Hermes middleware](https://hermes-agent.nousresearch.com/docs/developer-guide/middleware) | Request rewriting, execution wrappers, traces |
| [Hermes context engines](https://hermes-agent.nousresearch.com/docs/developer-guide/context-engine-plugin) | Replaceable context strategies |
| [Hermes curator](https://hermes-agent.nousresearch.com/docs/user-guide/features/curator) | Skill maintenance and rollback; usage is not quality evidence |
| [Claude plugin reference](https://code.claude.com/docs/en/plugins-reference) | Manifest and component mappings |
| [Claude mods](https://code.claude.com/docs/en/plugins/mods/reference) | TypeScript event middleware and host-specific APIs |
| [Claude Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview) | Comparison backend; it owns an existing agent loop |
| [Claude plugin evaluations](https://code.claude.com/docs/en/plugin-evals) | Baseline/candidate trials and independent checks |
| [Agent evaluation guidance](https://www.anthropic.com/research/demystifying-evals-for-ai-agents) | Outcome validation, repeat trials, evaluation design |
| [Cache diagnostics](https://platform.claude.com/docs/en/build-with-claude/cache-diagnostics) | Request divergence versus actual cache use |
| [MCP Rust SDK](https://github.com/modelcontextprotocol/rust-sdk) | Supported transports and protocol versions |

Do not copy vendor prompts or implementation code without checking the source license. Do not infer that HTTP tool access, plugin installation, or a subscription grants access to a vendor-hosted runtime.
