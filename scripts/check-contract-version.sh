#!/usr/bin/env bash
# Keeps one CONTRACT_VERSION to one trading account release (ft-core RFC "Contract Build and
# Release"). A release is an annotated tag `trading-account/vX.Y.Z` on its commit, whose message
# carries the release wasm's SHA-256 (docs/releases.md).
#
# Usage:
#   scripts/check-contract-version.sh
#       PRs and main. If the contract changed since the newest release tag, CONTRACT_VERSION must
#       be higher than that release's. Needs the tags fetched.
#   scripts/check-contract-version.sh <tag> <wasm>
#       Release tag pushes. The tag must name this commit's CONTRACT_VERSION, and its message must
#       carry <wasm>'s SHA-256 in hex.
#
# Comparing hashes on every PR wouldn't work: the release wasm records its commit (NEP-330), so
# every commit builds a different hash.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "::error::$1"
  exit 1
}

version=$(sed -nE 's/^const CONTRACT_VERSION: &str = "([^"]+)";.*/\1/p' contracts/trading-account/src/lib.rs)
[[ -n $version ]] || fail "CONTRACT_VERSION not found in contracts/trading-account/src/lib.rs"

if [[ $# -eq 2 ]]; then
  tag=$1
  wasm=$2
  [[ $tag == "trading-account/v$version" ]] || fail "$tag doesn't match CONTRACT_VERSION $version"
  [[ $(git cat-file -t "$tag") == tag ]] || fail "$tag isn't an annotated tag"
  hash=$(sha256sum "$wasm" | cut -d' ' -f1)
  git tag -l --format='%(contents)' "$tag" | grep -q "$hash" \
    || fail "$tag's message doesn't carry this build's SHA-256, $hash"
  echo "$tag is CONTRACT_VERSION $version, SHA-256 $hash"
  exit 0
fi

latest=$(git tag -l 'trading-account/v*' --sort=-v:refname | head -1)
if [[ -z $latest ]]; then
  echo "No trading account release yet; CONTRACT_VERSION $version is free"
  exit 0
fi
released=${latest#trading-account/v}

if [[ $version == "$released" ]]; then
  # What goes into the wasm. Cargo.lock is shared with the factory, so a factory-only dependency
  # change asks for a bump too; skipping a version number is harmless.
  if git diff --quiet "$latest" HEAD -- \
    rust-toolchain.toml contracts/Cargo.lock contracts/trading-account/Cargo.toml \
    contracts/trading-account/src \
    ':(exclude)contracts/trading-account/src/*tests.rs' \
    ':(exclude)contracts/trading-account/src/test_support.rs'; then
    echo "CONTRACT_VERSION $version is $latest, and the contract hasn't changed since"
    exit 0
  fi
  fail "The trading account changed since $latest. Bump CONTRACT_VERSION past $version (docs/releases.md#versions)."
fi

highest=$(printf '%s\n' "$released" "$version" | sort -V | tail -1)
[[ $highest == "$version" ]] || fail "CONTRACT_VERSION $version is below the newest release, $latest"
echo "CONTRACT_VERSION $version is new; the newest release is $latest"
