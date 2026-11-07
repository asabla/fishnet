# M1 fixtures

The versioned scripted provider lives in
`crates/fishnet-core/src/fixture.rs` (`scripted-v1`). The immutable tool catalog
is `fixture-catalog-v1`. Scripts assert the preceding tool result before they
return a final response. Expected values are separate from the tool source.

Core behavioral cases live in `crates/fishnet-core/tests/`. CLI process tests
live in `apps/cli/tests/cli.rs`. Recovery cases write sanitized journal prefixes
and partial tails. A process test kills the CLI after a tool result is saved,
then inspects it from another process and checks that the file is unchanged.

JSON events use schema version 1. Unsupported versions fail inspection. No live
API response, credential, user content, or browser session is used by these tests.
