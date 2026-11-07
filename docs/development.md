# Development policy

Use Rust 1.94.1. `rust-toolchain.toml` pins the compiler, rustfmt, and Clippy.
All workspace packages must inherit the workspace lint settings.
Commit `Cargo.lock`. Direct dependencies have exact versions.

## Quality command

Install the pinned toolchain and populate the dependency cache once. These
bootstrap steps can need network access. They do not need provider credentials.

```sh
make setup
make check
```

The script runs formatting, default tests, Clippy for every target, and rustdoc.
Cargo runs offline after bootstrap. The GitHub workflow calls the same script
on macOS and Linux. A workflow definition is not evidence that hosted checks passed.

## Make commands

Run `make` without a target to print help. All command targets are `.PHONY`, so
a file with the same name cannot prevent a command from running. `.PHONY` does
not print a message; the default `help` recipe prints the command list.

| Command | Action |
| --- | --- |
| `make setup` | Fetch locked dependencies; network access can be required |
| `make build` | Build the workspace offline |
| `make tui` | Open fishnet; start its local service if needed |
| `make daemon` | Start the local task service; use `ARGS` for host options |
| `make run` | Open fishnet; start its local service if needed |
| `make fixture` | Run the default offline JSON fixture |
| `make install` | Install the fishnet command from this checkout offline |
| `make cli ARGS="..."` | Pass arguments to the local CLI; no arguments opens fishnet in an interactive terminal |
| `make fmt` | Format Rust source files |
| `make fmt-check` | Check formatting without changes |
| `make test` | Run workspace tests offline |
| `make lint` | Run strict Clippy checks for all targets offline |
| `make docs` | Build API documentation offline with strict rustdoc checks |
| `make check` | Call the canonical `./scripts/check.sh` quality command |
| `make clean` | Remove Cargo artifacts; retain `.fishnet` runs and account files |

For fixture options, use `make fixture ARGS="--scenario negative"`. For inspection,
use `make cli ARGS="inspect RUN_ID --summary"`. `ARGS` is a local developer
command-line convenience. It does not change the core's tool authorization.
Live account and coding commands require explicit CLI arguments. Cargo's
`--offline` flag controls dependency access, not the application's provider I/O.

Make adds a discoverable entry point without a new task-runner dependency. The
check script remains the common quality policy for local development and CI.
Direct Cargo commands and `./scripts/check.sh` remain available without Make.

## Enforced Rust checks

Compiler warnings are errors. Unsafe code and non-ASCII identifiers are forbidden.
Clippy `all`, `pedantic`, and `nursery` are errors. Additional checks reject
unchecked indexing, `unwrap`, `expect`, explicit panic, placeholders, debug
macros, process exit calls, and unexplained lint suppression. Public fallible
functions document their errors. Rustdoc warnings and broken links are errors.
Release builds retain integer overflow checks.

The complexity threshold is 15. The argument threshold is 6. The function
length threshold is 80 lines as counted by Clippy. These are local review
thresholds. They are not proof of readability. M1 has no lint suppressions. M2 has three narrow `#[expect]` attributes for CLI
entry points. `unreachable_pub` requires crate visibility, while the pinned
Clippy `redundant_pub_crate` lint rejects it inside private binary modules. The
attributes record this conflict and become errors when the expectation is no
longer needed. Do not enable every restriction lint: several impose conflicting policies.

## Code intent

Apply the [Code with intent methodology](https://code-intent-evidence.emilsoderling.chatgpt.site/).
Its Rust guidance uses rustfmt, selected Clippy checks, and manual responsibility
review. Apply these rules to handwritten code:

- Keep an operation and its error check together.
- Separate unrelated statement groups with one blank line.
- Give functions names that describe their operation.
- Extract a function when it owns a useful responsibility.
- Use multiline control flow and narrow variable scopes.
- Explain constraints and reasons in comments.
- Review ownership, error propagation, evaluation order, and resource lifetime.

Rustfmt and Clippy cannot establish intent. Review the boundaries between policy,
provider invocation, tool execution, evidence checks, journal writes, and record
inspection. Do not split code only to satisfy a metric. Record a narrow exception
with `#[expect(..., reason = "...")]` when justified. Do not raise thresholds to
hide a finding. No accessibility interface is added by M1.

Use [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).
Keep documentation, contracts, runtime behavior, and acceptance evidence in
reviewable commits. Do not push without a user request.
