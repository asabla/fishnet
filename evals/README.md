# Task evaluations

This directory is reserved for evaluation definitions and sanitized cases. It contains no executable evaluator yet.

Each future case must specify: requested outcome; constraints; initial workspace/environment; completion checks protected from candidate edits; permitted tools; resource limits; baseline/candidate policy versions; model settings; repeated-trial policy; cache conditions; and expected evidence.

Include negative, failure, cancellation, ambiguous-intent, and source-change cases. Keep a held-out set separate from tuning cases. Report quality first, then latency, cost and unnecessary work. Fresh policy evaluations require new controlled runs; recorded-response replay tests only the behavior compatible with those recordings.

See [improvement loop](../docs/improvement-loop.md).
