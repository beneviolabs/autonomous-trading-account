#!/bin/bash
# DEV build of the factory into contracts/target/near/trading_account_factory/trading_account_factory.wasm.
# Not for deployment: release builds are reproducible, run `make release` from the repo root.
set -euo pipefail
cd "$(dirname "$0")"

NEAR_RUST_TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"/\1/p' ../../rust-toolchain.toml)"


# Check required tools
check_requirements() {
    # Check if NEAR CLI is installed
    if ! command -v near &> /dev/null; then
        echo "near CLI is not installed. Install near-cli-rs: https://github.com/near/near-cli-rs"
        exit 1
    fi

    # Check if wasm-opt is installed
    if ! command -v wasm-opt &> /dev/null; then
        echo "Installing wasm-opt via Homebrew..."
        if ! command -v brew &> /dev/null; then
            echo "Homebrew not found. Please install from https://brew.sh"
            exit 1
        fi
        brew install binaryen
    fi

    # Check if wasm32 target is installed for the pinned build toolchain.
    if ! rustup target list --installed --toolchain "$NEAR_RUST_TOOLCHAIN" | grep -q "wasm32-unknown-unknown"; then
        echo "Installing wasm32 target for Rust $NEAR_RUST_TOOLCHAIN toolchain..."
        rustup target add wasm32-unknown-unknown --toolchain "$NEAR_RUST_TOOLCHAIN"
    fi
}

# Run requirement checks
check_requirements

echo "Running cargo formatter "
cargo fmt

# Build the contract
echo "Building contract..."
../../scripts/build-wasm.sh .

WASM_PATH="../target/near/trading_account_factory/trading_account_factory.wasm"

# Verify WASM magic header after optimization
echo "Verifying WASM header..."
if ! xxd -p -l 4 "$WASM_PATH" | grep -q "0061736d"; then
    echo "❌ Invalid WASM header! Expected '0061736d' (\\0asm)"
    echo "First 4 bytes: $(xxd -p -l 4 "$WASM_PATH")"
    exit 1
else
    echo "✅ Valid WASM header verified"
fi
