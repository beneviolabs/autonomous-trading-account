#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
GREEN='\033[0;32m'
NC='\033[0m' # No Color

echo -e "${GREEN}Running Auth Proxy Contract Tests (Docker)...${NC}"
# Run unit tests only, skip integration tests that require sandbox
cargo test --lib -- --nocapture
echo ""

echo -e "${GREEN}Running Factory Contract Tests (Docker)...${NC}"
cargo test --manifest-path factory/Cargo.toml --lib -- --nocapture
echo ""

# Skip integration tests that require near-workspaces which fails in docker
# cargo test --features integration-tests --lib integration_tests -- --nocapture
echo ""
echo -e "${GREEN}All tests passed!${NC}"
