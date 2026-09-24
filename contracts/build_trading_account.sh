#!/usr/bin/env bash
set -euo pipefail

# near-sandbox currently accepts contract wasm produced by Rust 1.86 or older.
# https://github.com/near/near-workspaces-js/issues/225#issuecomment-1853577966

echo "Running cargo formatter "
cargo fmt

# Do not let cargo-near reuse an artifact produced by a different build mode.
rm -f target/near/proxy_contract.wasm

cargo near build non-reproducible-wasm --no-abi
