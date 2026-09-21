.PHONY: help run local build check fmt fmt-check lint test ci install clean

help:
	@printf '%s\n' \
		'Targets:' \
		'  make run        Run the client with configured connection settings' \
		'  make local      Run the client against localhost:3791' \
		'  make install    Install the release binary with cargo' \
		'  make build      Build the project' \
		'  make check      Run cargo check' \
		'  make fmt        Format Rust code' \
		'  make fmt-check  Verify Rust formatting' \
		'  make lint       Run clippy with warnings denied' \
		'  make test       Run tests' \
		'  make ci         Run fmt-check, lint, test, and diff whitespace checks' \
		'  make clean      Remove build artifacts'

run:
	cargo run

local:
	cargo run -- --local

build:
	cargo build

check:
	cargo check

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test

ci: fmt-check lint test
	git diff --check

install:
	cargo install --path . --locked

clean:
	cargo clean
