# Rust libraries

`fishnet-core` owns protocol types, fixture responses, the read-only tool, task
state, execution limits, completion checks, run journals, and local incidents.
It does not depend on the CLI or another agent runtime.

The CLI is the other workspace package. Other architecture components remain
future work. Add crate boundaries when a tested responsibility needs them.
