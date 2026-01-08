#!/bin/bash

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

    # Check if wasm32 target is installed for nightly
    if ! rustup target list --installed --toolchain nightly | grep -q "wasm32-unknown-unknown"; then
        echo "Installing wasm32 target for nightly toolchain..."
        rustup target add wasm32-unknown-unknown --toolchain nightly
    fi

    # Check if wasm32 target is installed for stable
    if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
        echo "Installing wasm32 target for stable toolchain..."
        rustup target add wasm32-unknown-unknown
    fi
}

# Run requirement checks
check_requirements

# Clear previous builds
echo "Clearing previous builds..."
cargo clean

echo "Running cargo formatter "
cargo fmt

# Set appropriate network based on NEAR_ENV
if [ "$NEAR_ENV" = "testnet" ]; then
  NETWORK="testnet"
else
  NETWORK="near"
fi
# Build the contract
echo "Building contract..."
RUSTFLAGS="-Z unstable-options" cargo +nightly near build non-reproducible-wasm --no-abi

# Set variables
# https://nearblocks.io/txns/FSP5xgN2dKg4dX64dMV3P92MnvhbRYPZHqZx2cdpvuUQ
#near contract deploy-as-global use-file /Users/charles/.nearai/registry/charleslavon.near/ft-allowance/contracts/target/near/proxy_contract.wasm as-global-hash peerfolio.testnet network-config testnet sign-with-keychain send
GLOBAL_PROXY_CODE_HASH="FTwNLjNXmku6hKVnXSP9Q9QmnwcTqzpG8dhFeoic5DsK"
WASM_PATH="target/near/proxy_factory.wasm"
FACTORY_ACCOUNT="auth-v2.peerfolio.$NETWORK"
FACTORY_OWNER="peerfolio.peerfolio.$NETWORK"
ROOT_ACCOUNT="peerfolio.$NETWORK"


# Verify WASM magic header after optimization
echo "Verifying WASM header..."
if ! xxd -p -l 4 "$WASM_PATH" | grep -q "0061736d"; then
    echo "❌ Invalid WASM header! Expected '0061736d' (\\0asm)"
    echo "First 4 bytes: $(xxd -p -l 4 "$WASM_PATH")"
    exit 1
else
    echo "✅ Valid WASM header verified"
fi

# Check if WASM file exists
if [ ! -f "$WASM_PATH" ]; then
    echo "Error: WASM file not found at $WASM_PATH"
    exit 1
fi


# Deploy factory
    echo "Deploying factory contract..."
if ! near state "$FACTORY_ACCOUNT" &>/dev/null; then
    near create-account "$FACTORY_ACCOUNT" --masterAccount "$ROOT_ACCOUNT" --initialBalance 4

    echo "Waiting 2 seconds for block finality before deploying..."
    sleep 2

    near deploy \
    "$FACTORY_ACCOUNT" \
    "$WASM_PATH" \
    --initFunction "new" \
    --initArgs '{"owner_id":"'"$FACTORY_OWNER"'", "network":"'"$NEAR_ENV"'", "global_proxy_base58_hash":"'"$GLOBAL_PROXY_CODE_HASH"'"}'
 else
    near deploy \
    "$FACTORY_ACCOUNT" \
    "$WASM_PATH"
fi


# Generate and verify checksum format
echo "Generating WASM checksum..."
WASM_CHECKSUM=$(shasum -a 256 "$WASM_PATH" | cut -d ' ' -f 1)
echo "WASM checksum (hex): 0x$WASM_CHECKSUM"

echo "Waiting 2 seconds for block finality before checksum verification..."
sleep 2
# Verify length is correct for SHA-256 (64 hex characters)
if [ ${#WASM_CHECKSUM} -eq 64 ]; then
    echo "✓ Checksum verified (32 bytes/64 hex characters)"
else
    echo "✗ Invalid checksum length"
    exit 1
fi

# Get deployed contract code hash
echo "Fetching deployed contract hash..."
DEPLOYED_HASH=$(near state "$FACTORY_ACCOUNT" | grep "Contract (SHA-256 checksum hex)" | awk '{print $NF}')

if [ -z "$DEPLOYED_HASH" ]; then
    echo "❌ Failed to fetch deployed contract hash"
    exit 1
fi

if [ "$WASM_CHECKSUM" != "$DEPLOYED_HASH" ]; then
    echo "❌ Checksum mismatch!"
    echo "Local WASM:    0x$WASM_CHECKSUM"
    echo "Deployed code: 0x$DEPLOYED_HASH"
    exit 1
else
    echo "✅ Checksum match confirmed"
fi


# Check deployment status
if [ $? -eq 0 ]; then
    echo "Factory Contract updated successfully at $FACTORY_ACCOUNT"
else
    echo "Factory Contract deployment failed"
    exit 1
fi
