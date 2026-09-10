.PHONY: test lint fmt test-rust lint-rust test-node lint-node test-py lint-py build-lambdas deploy tv-vvd-start tv-vvd-install tv-vvd-logs tv-vvd-shot tv-walkthrough

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

# Rust Lambdas for arm64 via cargo-lambda (`cargo install cargo-lambda`). Output lands in
# services/target/lambda/<name>/bootstrap, which the CDK stack packages.
build-lambdas:
	cd services && cargo lambda build --release --arm64 --bin api --bin ingest

deploy: build-lambdas
	cd infra && pnpm cdk deploy --require-approval never $(CDK_ARGS)

# Vega Virtual Device loop (needs the Vega SDK 0.24 with the `vega` CLI on PATH). Unverified
# on a device until the week 1 spike; see specs/001-memorylane-baseline/quickstart.md.
VVD ?= VirtualDevice
TV_APP_ID ?= com.hanyu.memorylane.main
TV_SHOT ?= docs/screens/vvd.png

tv-vvd-start:
	vega virtual-device start
	vega device start-port-forwarding --device $(VVD) -p 8081 --forward false

tv-vvd-install:
	cd apps/tv-vega && pnpm build:vega:debug
	vega run-app apps/tv-vega/build/aarch64-debug/*.vpkg $(TV_APP_ID) -d $(VVD)

tv-vvd-logs:
	vega device start-log-stream --device $(VVD)

# gwsi-tool-screenshooter runs only on a physical Fire TV (per the Vega CLI docs, not on the
# Virtual Device); for the Virtual Device capture its window on the host instead.
tv-vvd-shot:
	mkdir -p $(dir $(TV_SHOT))
	vega exec vda shell gwsi-tool-screenshooter /tmp/memorylane-shot.png
	vega device copy-from --device $(VVD) --source /tmp/memorylane-shot.png
	mv memorylane-shot.png $(TV_SHOT)

tv-walkthrough:
	scripts/tv-walkthrough.sh
