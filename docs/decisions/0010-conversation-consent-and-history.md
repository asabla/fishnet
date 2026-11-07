# ADR-0010: Conversation consent and bounded history

Status: accepted

Date: 2026-11-08

## Problem

Native screenshots from the user showed that entering a short message still
opened exact file selection and a large technical grant. The prior visual pass
did not correct that workflow. A prompt was not sufficient to make the service
support conversation.

## Decision

Keep the Rust loop, journals, service, client transport, and current dependencies.
Add a versioned text conversation context to the core contract. A chat turn has
no workspace, no tool schemas, and an explicit deny path for unexpected tool
calls. Its checker reports unavailable evidence and the outcome remains
unverified. Model final text cannot establish success.

The service accepts `Chat` submissions with an optional parent task ID. A root
turn waits for consent before any provider work. Consent covers messages in that
conversation only. A child requires a completed approved conversation parent and
the same provider configuration, account label, and requested model. Rejected,
failed, cancelled, interrupted, and truncated turns cannot pass on consent.
Changing that scope requires a new conversation. Branching from an explicitly
selected completed turn is permitted. Coding grants cannot become chat consent.

Freeze the full alternating user/assistant text context in each child receipt
and RunCreated event. Use at most 16 turns, 32 KiB of text, a 4096-byte user
message, and an 8192-byte response. Reject overflow; do not silently discard
history. Reconstruct the view from the journal without repeating model or tool
calls. These records are session data. They are not project memory, skills,
policy experiments, or automatic policy promotion.

The Responses adapter sends explicit role messages with `store=false`, no tools,
and `tool_choice=none`. It starts a fresh response request per user turn. It does
not use a hosted conversation store or retain hidden reasoning between user
turns. The existing coding loop retains its native provider items within a task.
The [official conversation state guide](https://developers.openai.com/api/docs/guides/conversation-state),
read on 2026-11-08, documents explicit user and assistant history. Mocked request
fixtures establish our request shape; they do not establish live account support.

Plain Enter submits chat. `/code` selects the existing explicit code task path;
code tasks remain separate. Default consent review is concise. F2 opens complete
grant details. The service still enforces the frozen grant revision. An
authenticated capability check detects an older service before sending chat.
The client does not stop or replace accepted work during an upgrade.

Add an explicit scripted conversation provider to test the complete service and
terminal path offline. It cannot run code tasks and never substitutes for a
failed live provider.

## Alternatives and limits

Inferring code access from prose would require a new authorization contract.
Reusing coding tasks for chat would retain unrelated Cargo and exact-file gates.
Both would keep the wrong interaction or expand authority. Text context is the
smallest reversible change. Streaming text, Markdown, inline diffs, and dynamic
coding permissions remain M4 work. No M5 implementation is included.

Existing receipts and journals decode with an absent conversation field. New
conversation records require a version that supports this contract. Reversal
must retain a reader for those records; it must never replay them.

## Evidence

See the conversation workflow section in [M4 evidence](../m4-evidence.md).
