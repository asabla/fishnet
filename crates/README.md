# Rust libraries

Create actual crates during implementation. Begin with only the boundaries required by M1: core execution and shared protocol types if separating them is justified.

Later responsibilities may include providers, tools, plugins, workspace, context, storage, diagnostics, and evaluation. These do not all need separate crates. The core must not depend on a client UI. Keep transport adapters outside the execution semantics.
