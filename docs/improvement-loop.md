# Improving execution policy

Status: required behavior with a proposed implementation sequence.

## Objects

| Record | Meaning |
| --- | --- |
| Observation | An execution fact with provenance |
| Incident | A selected problem worth investigating |
| Hypothesis | A testable explanation with uncertainty |
| Experiment | A candidate change, baseline, cases, and results |
| Policy release | An accepted version, scope, evidence, and rollback target |

Keep these separate from user/project memory. A unique incident can become a regression case without becoming a general instruction.

## Recovery within a task

1. Establish a small task contract: outcome, constraints, completion evidence, unresolved questions, and policy version.
2. Record operations and observable progress. Optional purpose labels describe operational intent, not hidden model reasoning.
3. Detect repeated equivalent calls, retries without changed prerequisites, repeated reads of unchanged content, patch/revert cycles, or long gaps between verified milestones.
4. Use cheap counters and fingerprints first. Trigger bounded diagnosis only when evidence warrants it.
5. Revisit the task contract, established facts, and unsupported assumptions. Select a materially different action with an explicit expected observation.
6. Ask a focused user question only if unresolved intent changes the deliverable. Cap intervention attempts and record their cost.

A nonzero exit status can be expected evidence. A test exposing a defect or a search eliminating an explanation can represent progress. Many calls alone do not prove poor understanding.

## Improvement across tasks

1. Select an incident and preserve a sanitized reproducible starting state.
2. Record a cause hypothesis, affected scope, and confidence.
3. Propose one bounded policy or implementation change.
4. Compare baseline and candidate on the incident, related cases, and held-out cases.
5. Repeat stochastic trials. Record model, provider, parameters, tool versions, environment, repository snapshots, and policy versions. Account for warm/cold caches and changing network conditions.
6. Require task success and quality before comparing efficiency. Record uncertainty and regressions by task family.
7. Reject, retain for more evidence, or promote a versioned change. Keep rollback and monitor subsequent results.

Do not remove failed, cancelled, or unresolved trials. Evaluation checks must be protected from candidate modifications. A trace replay can test event handlers; a changed retrieval/planning policy requires fresh runs in a controlled environment.

## Initial diagnostic rules

- Equivalent calls against unchanged relevant state are repetition candidates, not automatic failures.
- Distinguish schema errors, missing prerequisites, expected negative evidence, permission denials, transient transport failures, and unknown side effects.
- Compare request fingerprints by section to identify cache-prefix changes. Distinguish provider-reported evidence from local comparison and inferred causes.
- Count model round trips, logical tool operations, subprocess work, tokens, and elapsed time separately. Batching cannot hide work from the metrics.

## Promotion

Initial releases only produce candidate changes and reports. Do not automatically edit source, install plugins, or promote policies. Later automatic promotion is a separate decision limited to tested, reversible changes with regression checks and a resource budget. Permissions and task-success criteria cannot be relaxed to improve scores.

## Acceptance

A detector can identify an unchanged invalid retry without classifying an expected failing test as waste. An experiment can show a policy change's effect without writing a user memory. Unknown outcomes remain unknown. A rejected candidate does not change live behavior.
