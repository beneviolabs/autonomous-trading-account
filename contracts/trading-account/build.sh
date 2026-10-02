#!/usr/bin/env bash
# DEV build of the trading account into contracts/target/near/trading_account/trading_account.wasm.
# Not for deployment: release builds are reproducible, run `make release` from the repo root.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Running cargo formatter "
cargo fmt

../../scripts/build-wasm.sh .
