#!/usr/bin/env bash
# Builds one contract crate's wasm. Local scripts and Docker/CI both use it.
#
# Usage: ./build_wasm.sh <crate dir> <wasm file name>
#
# The canonical, reproducible hash (the one for DAO proposals) comes from running this in
# the linux/amd64 Docker image: `make docker-build-contracts`. A native build on another
# host (e.g. macOS arm64) still produces different code, so its hash won't match.
#
# - Rust and cargo-near versions are pinned (cargo-near bundles wasm-opt).
# - $CARGO_HOME is remapped, because panic locations embed dependency source paths
#   (/Users/<you>/.cargo/registry/... locally, /root/.cargo/registry/... in Docker), which
#   would otherwise make even Docker builds depend on the user running them.
set -euo pipefail

NEAR_RUST_TOOLCHAIN="${NEAR_RUST_TOOLCHAIN:-1.85.0}"
CARGO_NEAR_VERSION="0.16.0"

crate_dir="$1"
wasm_name="$2"

installed="$(cargo near --version | awk '{print $2}')"
if [ "$installed" != "$CARGO_NEAR_VERSION" ]; then
    echo "cargo-near $CARGO_NEAR_VERSION is required, found $installed." >&2
    echo "Run: cargo install cargo-near --version $CARGO_NEAR_VERSION --locked" >&2
    exit 1
fi

cd "$crate_dir"

# Do not let cargo-near reuse an artifact produced by a different build mode.
rm -f "target/near/$wasm_name"

cargo "+$NEAR_RUST_TOOLCHAIN" near build non-reproducible-wasm --no-abi \
    --env "RUSTFLAGS=--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"

# cargo-near writes the wasm as 0600; in Docker that file is owned by root.
chmod a+r "target/near/$wasm_name"

if command -v sha256sum >/dev/null; then
    sha256sum "target/near/$wasm_name"
else
    shasum -a 256 "target/near/$wasm_name"
fi
