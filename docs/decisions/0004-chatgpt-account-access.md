# ADR-0004: Direct ChatGPT account authorization

Status: accepted

Date: 2026-11-07

## Problem and constraints

The user selected ChatGPT account access. Fishnet must retain its own execution
loop and remain open to other model providers. It must not borrow another
application's credential files or use another agent as an execution engine.

## Decision

Implement the documented direct public-client OAuth flow with PKCE, a random
state and nonce, and a loopback callback. Use fishnet's actual name for initial
registration. Retain the issued client ID and a stable installation host ID.
Do not reuse Codex credentials or ChatGPT browser cookies in the runtime.

Validate the ID token with the issuer's JWKS before accepting identity. Pin
jsonwebtoken 10.4.0 with its Rust crypto backend, base64 0.22.1, and getrandom
0.2.17. Validate signature, algorithm, issuer, audience, expiry, nonce, subject,
and issued-at time. Permit RS256 and ES256. An unsupported algorithm fails.
A valid identity alone does not permit plan use; require the granted direct-use
scope. The service determines account and application eligibility.

Store credentials under the ignored `.fishnet/auth` directory by default. Use
Unix mode 0700 for the directory and 0600 for files. Reject symlinks and broad
permissions. Save replacements atomically. A file lock serializes refreshes.
Record uncertain refresh before requesting renewal; an unknown outcome requires
fresh sign-in rather than a blind rotating-token retry. Keep account labels and
verified subjects separate. Logout clears tokens while retaining registration
metadata and reports whether remote revocation was confirmed.

Use the public Responses endpoint for plan usage. Query the account's model
catalog and preserve its ordering. ChatGPT mode omits unsupported parameters,
uses namespace tools, streams responses, and disables server response storage.
Do not redirect account credentials to a custom endpoint or silently switch to
API-key billing. Other providers use their own explicit credential source.

## Evidence and validation

Use the official [sign-in registration contract](https://developers.openai.com/siwc/token-sharing-open-source/sign-in),
[account lifecycle contract](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions),
[model and inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference),
and [preview limits](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).

Default tests use disposable signing material generated for these fixtures.
It is not an account credential. Tests reject altered signatures and claims,
wrong callback state/client IDs, unsafe files, missing permission, and uncertain
renewal. The real sign-in flow completed on the development host after the user
approved it. Inference qualification is recorded separately in [M2 evidence](../m2-evidence.md).

## Consequences and compatibility

The credential file is protected by filesystem permissions, not encrypted at
rest. Hostile processes running as the same OS user are outside this protection.
No credential is added to a task contract, model prompt, or run journal.
Credentials are not user/project memory. No account or license entitlement is
inferred from the repository being private or from a successful identity login.

## Reversal or migration path

Replace file storage with a platform credential store without changing the
provider or execution loop. Add another authentication method behind an explicit
provider route. Recheck the preview contract before changing transport fields.
