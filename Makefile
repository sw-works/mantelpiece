.PHONY: test lint fmt test-rust lint-rust test-node lint-node test-py lint-py build-lambdas build-agent deploy deploy-web up down tv-vvd-start tv-vvd-install tv-vvd-logs tv-vvd-shot tv-vvd-stop tv-walkthrough

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
	cd services && cargo lambda build --release --arm64 --bin api --bin ingest --bin poster --bin recording

# The Concierge agent for AgentCore Runtime: its code plus arm64 wheels in
# agents/build/runtime, which the CDK stack zips. Needs uv.
build-agent:
	scripts/build-agent.sh

deploy: build-lambdas build-agent
	cd infra && pnpm cdk deploy --require-approval never $(CDK_ARGS)

# Second pass: build the phone pages against the deployed stack's outputs and publish them.
# `make deploy` first (the pages need the API URL), then this; repeat for page changes.
deploy-web:
	scripts/deploy-web.sh $(CDK_ARGS)

# The stack is stood up for a test session and torn down after; nothing stays running
# between sessions. `up` is deploy + pages + TV config; `down` destroys the stack. The
# table, media bucket and user pool are retained (near-zero idle cost) unless both were run
# with CDK_ARGS="-c retain-data=false"; a fresh `up` does not reattach retained stores.
up: deploy deploy-web
	scripts/tv-config.sh

down:
	scripts/ivs-sweep.sh
	cd infra && pnpm cdk destroy --force $(CDK_ARGS)
	@echo "Stack gone. Retained stores: aws dynamodb list-tables; aws cognito-idp list-user-pools --max-results 20; aws s3 ls"

# Vega Virtual Device loop (needs the Vega SDK 0.24 with the `vega` CLI on PATH). Unverified
# on a device until the week 1 spike; see specs/001-mantelpiece-baseline/quickstart.md.
VVD ?= VirtualDevice
TV_APP_ID ?= com.hanyu.mantelpiece.main
TV_SHOT ?= docs/screens/vvd.png

tv-vvd-start:
	vega virtual-device start
	vega device start-port-forwarding --device $(VVD) -p 8081 --forward false

tv-vvd-install:
	cd apps/tv-vega && pnpm build:vega:debug
	vega run-app apps/tv-vega/build/aarch64-debug/*.vpkg $(TV_APP_ID) -d $(VVD)

tv-vvd-logs:
	vega device start-log-stream --device $(VVD)

# gwsi-tool-screenshooter hangs on the Virtual Device (physical Fire TV only) and a QEMU
# screendump comes back black; the emulator console on port 5554 renders on the host.
tv-vvd-shot:
	scripts/vvd-screenshot.sh $(TV_SHOT)

tv-vvd-stop:
	vega virtual-device stop

tv-walkthrough:
	scripts/tv-walkthrough.sh
