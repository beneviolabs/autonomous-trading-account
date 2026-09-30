#!/usr/bin/env bash
set -euo pipefail

# near-sandbox currently accepts contract wasm produced by Rust 1.86 or older.
# https://github.com/near/near-workspaces-js/issues/225#issuecomment-1853577966

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Running cargo formatter "
cargo fmt

./build_wasm.sh . proxy_contract.wasm
