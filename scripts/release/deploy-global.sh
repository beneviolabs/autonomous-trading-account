#!/usr/bin/env bash
# Deploys a trading account release as a global contract.
#
# Usage: scripts/release/deploy-global.sh <mainnet|testnet> <version> [--dry-run]
#
# Downloads the version's GitHub Release and checks its hash. Runs the sandbox tests against that
# wasm, which fetch a live 1Click quote; the tests come from this checkout, so its CONTRACT_VERSION
# must be the release's. Then deploys it with deploy-as-global, signed from the keychain as
# dev.peerfolio.near (or peerfolio.peerfolio.testnet), which pays for the storage: about 40 NEAR on
# mainnet. Does nothing if the global already exists.
# shellcheck source=../lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/../lib.sh"

args=()
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        -*) usage ;;
        *) args+=("$arg") ;;
    esac
done
[ "${#args[@]}" = 2 ] || usage
network="${args[0]}" version="${args[1]}"
deployer="$(global_deployer "$network")"

fetch_release "$version"

if global_exists "$network" "$RELEASE_BS58"; then
    info "$version ($RELEASE_BS58) is already a global contract on $network. Nothing to do."
    exit 0
fi
checkout_version="$(sed -nE 's/^const CONTRACT_VERSION: &str = "([^"]+)".*/\1/p' "$REPO_ROOT/contracts/trading-account/src/lib.rs")"
[ "$checkout_version" = "$version" ] \
    || die "this checkout's tests are for $checkout_version, not $version: check out a commit at $version, such as its tag"
sandbox_test "$RELEASE_WASM"

info "deploying $version as a global contract on $network, paid by $deployer"
run_tx near contract deploy-as-global use-file "$RELEASE_WASM" as-global-hash "$deployer" \
    network-config "$network" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

global_exists "$network" "$RELEASE_BS58" || die "$RELEASE_BS58 isn't a global contract on $network after the deploy"
info "done: $version is global contract $RELEASE_BS58 on $network"
