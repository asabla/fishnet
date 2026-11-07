# OAuth verification fixtures

`oauth-test-private.der` is a test-only RSA signing key. `oauth-test-jwks.json`
contains its public key. Tests use them to generate valid and invalid ID tokens
without a network request. These files are public fixtures. They are not account
credentials and must never be used for authentication outside the tests.
