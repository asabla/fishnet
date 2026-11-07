# M1 fixture cases

The executable cases are in `crates/fishnet-core/tests/m1.rs`. They check the
runtime against a fixed fixture contract. They are deterministic harness tests,
not measurements of a live model's planning quality.

Cases cover verified success, unsupported final claims, contradictory evidence,
provider failure, expected negative evidence, invalid repeated requests,
recovery after invalid requests, policy denial, cancellation, deadlines, budgets,
bounded output, record corruption, and interrupted operations. Failures,
cancellations, and incomplete outcomes stay in local run records. Tool-call
counts describe work; they do not decide success.

Fresh trials, holdout comparisons, and policy experiments remain M7 work.
