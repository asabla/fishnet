# Rust libraries

`fishnet-core` owns the execution loop, typed state, policy, providers, tools,
checks, journals, and local diagnostics. It does not depend on a client or an
external agent runtime.

`fishnet-service`, under `services/daemon`, adds durable task coordination,
authenticated transport, and a shared client. CLI and TUI service commands
use that contract. The CLI package retains explicit standalone M1/M2 utilities.
Add crate boundaries when a tested responsibility needs them.
