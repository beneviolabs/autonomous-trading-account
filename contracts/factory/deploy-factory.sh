#!/bin/bash

# Usage: ./deploy-factory.sh [FACTORY_OWNER] [GLOBAL_TRADING_ACCOUNT_BS58_HASH] [NETWORK]
# Example: ./deploy-factory.sh testnet dao.peerfolio.testnet FTwNLjNXmku6hKVnXSP9Q9QmnwcTqzpG8dhFeoic5DsK


# Validate required arguments
if [ -z "$1" ]; then
    echo "Error: FACTORY_OWNER is required"
    echo "Usage: ./deploy-factory.sh <FACTORY_OWNER> <GLOBAL_TRADING_ACCOUNT_BS58_HASH> <NETWORK> "
    echo "Example: ./deploy-factory.sh dao.peerfolio.testnet FTwNLjNXmku6hKVnXSP9Q9QmnwcTqzpG8dhFeoic5DsK"
    exit 1
fi

if [ -z "$2" ]; then
    echo "Error: GLOBAL_TRADING_ACCOUNT_BS58_HASH is required"
    echo "Usage: ./deploy-factory.sh <FACTORY_OWNER> <GLOBAL_TRADING_ACCOUNT_BS58_HASH> <NETWORK>"
    echo "Example: ./deploy-factory.sh dao.peerfolio.testnet FTwNLjNXmku6hKVnXSP9Q9QmnwcTqzpG8dhFeoic5DsK"
    exit 1
fi

FACTORY_OWNER="$1"
GLOBAL_TRADING_ACCOUNT_BS58_HASH="$2"
NETWORK="${3:-testnet}" # default to testnet if not specified

echo "Using NETWORK: $NETWORK"
echo "Using FACTORY_OWNER: $FACTORY_OWNER"
echo "Using GLOBAL_TRADING_ACCOUNT_BS58_HASH: $GLOBAL_TRADING_ACCOUNT_BS58_HASH"

# Set variables
WASM_PATH="target/near/proxy_factory.wasm"
FACTORY_ACCOUNT="auth.peerfolio.$NETWORK"
ROOT_ACCOUNT="peerfolio.$NETWORK"

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
    --initArgs '{"owner_id":"'"$FACTORY_OWNER"'", "network":"'"$NETWORK"'", "global_proxy_base58_hash":"'"$GLOBAL_TRADING_ACCOUNT_BS58_HASH"'"}'
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
