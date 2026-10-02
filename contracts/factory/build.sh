#!/usr/bin/env bash
# DEV build of the factory into contracts/target/near/trading_account_factory/trading_account_factory.wasm.
# Not for deployment: release builds are reproducible, run `make release` from the repo root.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Running cargo formatter "
cargo fmt

../../scripts/build-wasm.sh .
