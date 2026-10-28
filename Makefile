.PHONY: release build fmt-check clippy shellcheck test-unit test-integration test audit help

# Packages are checked one at a time: the two contracts enable different near-sdk features,
# and a --workspace invocation would unify them.
PACKAGES = trading-account trading-account-factory

# Reproducible release build of both contracts in NEAR's pinned Docker image (see
# [package.metadata.near.reproducible_build] in each Cargo.toml). Needs Docker and a clean,
# committed git tree. These hashes are the ones to deploy and to put in DAO proposals.
release:
	cd contracts/trading-account && cargo near build reproducible-wasm
	cd contracts/factory && cargo near build reproducible-wasm

# Fast native dev builds. Never deploy these.
build:
	./scripts/build-wasm.sh contracts/trading-account
	./scripts/build-wasm.sh contracts/factory

fmt-check:
	cd contracts && cargo fmt --all -- --check

# --all-targets also lints the tests, and turns on near-sdk's unit-testing feature (a
# dev-dependency) that a host build of a contract needs.
clippy:
	cd contracts && for p in $(PACKAGES); do cargo clippy -p $$p --all-targets -- -D warnings || exit 1; done

shellcheck:
	shellcheck scripts/*.sh scripts/release/*.sh scripts/dev/*.sh

test-unit:
	cd contracts && for p in $(PACKAGES); do cargo test -p $$p --lib || exit 1; done

# The sandbox tests embed the trading account wasm at compile time, so it's rebuilt first. That
# build also trusts the key the tests sign quotes with (seed "1click", as in the unit tests), so
# never deploy it; scripts/dev/flash-dev.sh refuses to.
test-integration:
	ONE_CLICK_TEST_KEY="ed25519:4xzHWfzwdwnkGbsL3G1C7UBt1v9DX2ZnnE8zmatq3Lba" ./scripts/build-wasm.sh contracts/trading-account
	cd contracts && cargo test -p trading-account --features integration-tests --lib integration_tests

test: test-unit test-integration

# Ignores (with reasons) are in contracts/.cargo/audit.toml.
audit:
	cd contracts && cargo audit

help:
	@echo "Available commands:"
	@echo "  release    - Reproducible release build of both contracts (Docker, clean git tree)"
	@echo "  build      - Fast native dev build of both contracts (not for deployment)"
	@echo "  fmt-check  - Check formatting"
	@echo "  clippy     - Run clippy lints"
	@echo "  shellcheck - Lint the scripts"
	@echo "  test-unit  - Unit tests for both contracts"
	@echo "  test-integration - Rebuild the trading account and run its sandbox tests"
	@echo "  test       - Unit + integration tests"
	@echo "  audit      - cargo audit of the workspace lockfile"
