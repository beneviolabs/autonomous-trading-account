#!/usr/bin/env bash
# Runs all tests locally: unit tests for both contracts, then the trading account's sandbox
# integration tests. The integration tests embed
# contracts/target/near/trading_account/trading_account.wasm at compile time, so the contract
# is built first. CI runs only the unit tests (`make test-unit`).
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/contracts"

GREEN='\033[0;32m'
NC='\033[0m' # No Color

# One package per invocation: the contracts enable different near-sdk features, and testing
# them together (--workspace) would unify those features across both.
echo -e "${GREEN}Running Trading Account Contract Tests...${NC}"
cargo test -p trading-account --lib -- --nocapture
echo ""

echo -e "${GREEN}Running Factory Contract Tests...${NC}"
cargo test -p trading-account-factory --lib -- --nocapture
echo ""

echo -e "${GREEN}Running Integration Tests...${NC}"
"$ROOT/contracts/trading-account/build.sh"
cargo test -p trading-account --features integration-tests --lib integration_tests -- --nocapture

echo -e "${GREEN}All tests passed!${NC}"
