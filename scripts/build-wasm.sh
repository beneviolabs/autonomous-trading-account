#!/usr/bin/env bash
# Fast, native DEV build of one contract crate's wasm (used by `make build`, each crate's
# build.sh and scripts/test.sh).
#
# Usage: scripts/build-wasm.sh <crate dir>
# Output: contracts/target/near/<lib name>/<lib name>.wasm, even if CARGO_TARGET_DIR is set,
# because the integration tests (include_bytes!), deploy.sh and the docs all read it from there.
#
# Do NOT deploy what this produces. Its hash depends on the host (OS/CPU/toolchain), so it
# can't be verified by anyone else. Release builds are reproducible and run in NEAR's pinned
# Docker image: `make release` (cargo near build reproducible-wasm, configured under
# [package.metadata.near.reproducible_build] in each crate's Cargo.toml).
#
# - The Rust version comes from rust-toolchain.toml; cargo-near's version is pinned below.
# - $CARGO_HOME is remapped because panic locations embed dependency source paths
#   (/Users/<you>/.cargo/registry/...), which would otherwise vary per user.
set -euo pipefail

CARGO_NEAR_VERSION="0.16.0"

crate_dir="$1"
export CARGO_TARGET_DIR
CARGO_TARGET_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../contracts" && pwd)/target"

installed="$(cargo near --version | awk '{print $2}')"
if [ "$installed" != "$CARGO_NEAR_VERSION" ]; then
    echo "cargo-near $CARGO_NEAR_VERSION is required, found $installed." >&2
    echo "Run: cargo install cargo-near --version $CARGO_NEAR_VERSION --locked" >&2
    exit 1
fi

cd "$crate_dir"
lib_name="$(cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json,sys,os; m=json.load(sys.stdin); d=os.getcwd(); print(next(t["name"] for p in m["packages"] if os.path.dirname(p["manifest_path"])==d for t in p["targets"] if "cdylib" in t["kind"]))')"
wasm="$CARGO_TARGET_DIR/near/$lib_name/$lib_name.wasm"

# Do not let cargo-near reuse an artifact produced by a different build mode.
rm -f "$wasm"

cargo near build non-reproducible-wasm --no-abi \
    --env "RUSTFLAGS=--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"

if command -v sha256sum >/dev/null; then
    sha256sum "$wasm"
else
    shasum -a 256 "$wasm"
fi
