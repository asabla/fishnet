.DEFAULT_GOAL := help

.PHONY: help setup build install run fixture cli fmt fmt-check test lint docs check clean daemon tui

help: ## Show development commands and examples
	@printf '%s\n\n' 'fishnet development commands'
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z][a-zA-Z0-9_-]*:.*## / { printf "  make %-12s %s\n", $$1, $$2 }' Makefile
	@printf '\n%s\n' 'Examples:'
	@printf '%s\n' '  make run' '  make install  # then run fishnet from your project' '  make check' '  make fixture ARGS="--scenario negative"' '  make cli ARGS="tasks"'

setup: ## Fetch locked dependencies (network access can be required)
	cargo fetch --locked

build: ## Build the workspace offline
	cargo build --workspace --offline --locked

install: ## Install the fishnet command from this checkout offline
	cargo install --path apps/cli --offline --locked --force

run: ## Open fishnet; start the local service automatically
	cargo run --offline --locked -p fishnet-cli -- $(ARGS)

fixture: ## Run an offline JSON fixture; pass options with ARGS="..."
	cargo run --offline --locked -p fishnet-cli -- run $(ARGS)

cli: ## Run a command with ARGS="..."; no arguments opens fishnet in a terminal
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

tui: ## Open the terminal interface; start the local service automatically
	cargo run --offline --locked -p fishnet-cli -- tui $(ARGS)
