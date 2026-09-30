.PHONY: docker-build docker-run docker-test docker-clean help

# Build contracts with the Rust version supported by near-sandbox/nearcore.
NEAR_RUST_TOOLCHAIN ?= 1.85.0

# Docker commands for faster CI actions
docker-build:
	docker build -t near-contract-builder .

docker-test:
	docker run --rm -v $(PWD):/workspace -w /workspace near-contract-builder bash -c "cd contracts && ./test-docker.sh"

docker-fmt-check:
	docker run --rm -v $(PWD):/workspace -w /workspace near-contract-builder bash -c "cd contracts && cargo fmt -- --check"

docker-clippy:
	docker run --rm -v $(PWD):/workspace -w /workspace near-contract-builder bash -c "cd contracts && cargo clippy -- -D warnings"

docker-audit:
	docker run --rm -v $(PWD):/workspace -w /workspace near-contract-builder bash -c "cd contracts && cargo audit && cd factory && cargo audit"

# Build contracts with the same script used locally, so the wasm hashes match.
docker-build-contracts:
	docker run --rm -v $(PWD):/workspace -w /workspace near-contract-builder bash -c "cd contracts && NEAR_RUST_TOOLCHAIN=$(NEAR_RUST_TOOLCHAIN) ./build_wasm.sh . proxy_contract.wasm && NEAR_RUST_TOOLCHAIN=$(NEAR_RUST_TOOLCHAIN) ./build_wasm.sh factory proxy_factory.wasm"

docker-clean:
	docker system prune -f
	docker volume prune -f

# Local Debugging of Docker container
local-docker-run:
	docker run --rm -it -v $(PWD):/workspace -w /workspace near-contract-builder bash


help:
	@echo "Available commands:"
	@echo "  docker-build          - Build the Docker image"
	@echo "  docker-test           - Run tests in Docker"
	@echo "  docker-build-contracts - Build contracts in Docker"
	@echo "  docker-fmt-check      - Check code formatting in Docker"
	@echo "  docker-clippy         - Run clippy lints in Docker"
	@echo "  docker-audit          - Run security audit in Docker"
	@echo "  docker-clean          - Clean Docker system and volumes"
	@echo "  local-docker-run      - Run interactive Docker container"
