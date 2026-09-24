#!/bin/bash
set -euo pipefail

NEAR_RUST_TOOLCHAIN="${NEAR_RUST_TOOLCHAIN:-1.85.0}"


# Check required tools
check_requirements() {
    # Check if NEAR CLI is installed
    if ! command -v near &> /dev/null; then
        echo "NEAR CLI is not installed. Please install it first with: npm install -g near-cli"
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

# Clear previous builds
echo "Clearing previous builds..."
cargo clean

echo "Running cargo formatter "
cargo fmt

# Build the contract
echo "Building contract..."
cargo "+$NEAR_RUST_TOOLCHAIN" near build non-reproducible-wasm --no-abi

WASM_PATH="target/near/proxy_factory.wasm"

# Verify WASM magic header after optimization
echo "Verifying WASM header..."
if ! xxd -p -l 4 "$WASM_PATH" | grep -q "0061736d"; then
    echo "❌ Invalid WASM header! Expected '0061736d' (\\0asm)"
    echo "First 4 bytes: $(xxd -p -l 4 "$WASM_PATH")"
    exit 1
else
    echo "✅ Valid WASM header verified"
fi
