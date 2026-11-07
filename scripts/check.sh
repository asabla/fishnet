#!/bin/sh
set -eu

cargo fmt --all -- --check
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo doc --workspace --no-deps --offline --locked
