.PHONY: release build fmt-check clippy test-unit test audit help

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

# The factory is excluded until its redundant `use bs58;` is removed.
clippy:
	cd contracts && cargo clippy -p trading-account -- -D warnings

test-unit:
	cd contracts && for p in $(PACKAGES); do cargo test -p $$p --lib || exit 1; done

# Unit + sandbox integration tests.
test:
	./scripts/test.sh

# Ignores (with reasons) are in contracts/.cargo/audit.toml.
audit:
	cd contracts && cargo audit

help:
	@echo "Available commands:"
	@echo "  release    - Reproducible release build of both contracts (Docker, clean git tree)"
	@echo "  build      - Fast native dev build of both contracts (not for deployment)"
	@echo "  fmt-check  - Check formatting"
	@echo "  clippy     - Run clippy lints"
	@echo "  test-unit  - Unit tests for both contracts"
	@echo "  test       - Unit + integration tests"
	@echo "  audit      - cargo audit of the workspace lockfile"
