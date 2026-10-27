#!/usr/bin/env bash
# Deploys a trading account release as a global contract.
#
# Usage: scripts/release/deploy-global.sh <mainnet|testnet> <version> [--dry-run]
#
# Downloads the version's GitHub Release and checks its hash, then deploys it with
# deploy-as-global, signed from the keychain as peerfolio.near (or peerfolio.testnet). On mainnet
# that costs about 40 NEAR for storage. Does nothing if the global already exists.
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

info "deploying $version as a global contract on $network, paid by $deployer"
run_tx near contract deploy-as-global use-file "$RELEASE_WASM" as-global-hash "$deployer" \
    network-config "$network" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

global_exists "$network" "$RELEASE_BS58" || die "$RELEASE_BS58 isn't a global contract on $network after the deploy"
info "done: $version is global contract $RELEASE_BS58 on $network"
