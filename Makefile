.PHONY: test lint fmt test-rust lint-rust test-node lint-node test-py lint-py

test: test-rust test-node test-py
lint: lint-rust lint-node lint-py

test-rust:
	cd services && cargo test --workspace --all-features

lint-rust:
	cd services && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings

test-node:
	pnpm -r --if-present test

lint-node:
	pnpm -r --if-present lint && pnpm -r --if-present typecheck

test-py:
	cd agents && uv run --group dev pytest -q

lint-py:
	cd agents && uv run --group dev ruff check . && uv run --group dev ruff format --check .

fmt:
	cd services && cargo fmt --all
	cd agents && uv run --group dev ruff format .
