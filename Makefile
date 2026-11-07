.DEFAULT_GOAL := help

.PHONY: help setup build run cli fmt fmt-check test lint docs check clean daemon tui

help: ## Show development commands and examples
	@printf '%s\n\n' 'fishnet development commands'
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z][a-zA-Z0-9_-]*:.*## / { printf "  make %-12s %s\n", $$1, $$2 }' Makefile
	@printf '\n%s\n' 'Examples:'
	@printf '%s\n' '  make check' '  make daemon  # first terminal' '  make tui     # second terminal' '  make run ARGS="--scenario negative"' '  make cli ARGS="inspect RUN_ID --summary"'

setup: ## Fetch locked dependencies (network access can be required)
	cargo fetch --locked

build: ## Build the workspace offline
	cargo build --workspace --offline --locked

run: ## Run an offline fixture; pass fixture options with ARGS="..."
	cargo run --offline --locked -p fishnet-cli -- run $(ARGS)

cli: ## Run the CLI with ARGS="..."; no arguments shows CLI help
	cargo run --offline --locked -p fishnet-cli -- $(ARGS)

fmt: ## Format Rust source files
	cargo fmt --all

fmt-check: ## Check Rust formatting without changing files
	cargo fmt --all -- --check

test: ## Run all workspace tests offline
	cargo test --workspace --offline --locked

lint: ## Run strict Clippy checks for all targets offline
	cargo clippy --workspace --all-targets --offline --locked -- -D warnings

docs: ## Build API documentation offline with strict rustdoc checks
	cargo doc --workspace --no-deps --offline --locked

check: ## Run the complete quality checks used by CI
	./scripts/check.sh

clean: ## Remove Cargo build artifacts; keep local runs and account files
	cargo clean

daemon: ## Start the local task service; host options use ARGS="..."
	cargo run --offline --locked -p fishnet-service --bin fishnetd -- $(ARGS)

tui: ## Open the terminal client; start make daemon in another terminal
	cargo run --offline --locked -p fishnet-tui -- $(ARGS)
