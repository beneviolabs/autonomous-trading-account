#!/usr/bin/env bash
# Points the dev or testnet factory at a trading account release.
#
# Usage: scripts/release/point-factory.sh <dev|testnet> <version|v0> [--rollback] [--dry-run]
#
# Calls set_global_code_hash as the factory owner, from the keychain, and reads the pointer back.
# Refuses to move to an older version unless --rollback is given. The prod factory moves only
# through the DAO: scripts/release/dao-propose.sh pointer.
# shellcheck source=../lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/../lib.sh"

args=() rollback=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        --rollback) rollback=1 ;;
        -*) usage ;;
        *) args+=("$arg") ;;
    esac
done
[ "${#args[@]}" = 2 ] || usage
target="${args[0]}" version="${args[1]}"
case "$target" in dev|testnet) ;; *) usage ;; esac
factory_target "$target"

if [ "$version" = v0 ]; then
    RELEASE_BS58="$V0_HASH"
else
    fetch_release "$version"
fi
global_exists "$NETWORK" "$RELEASE_BS58" \
    || die "$version isn't a global contract on $NETWORK yet: scripts/release/deploy-global.sh $NETWORK $version"

current="$(view "$NETWORK" "$FACTORY" get_proxy_code_base58_hash | jq -r .)"
current_version="$(version_of_hash "$current")"
if [ "$current" = "$RELEASE_BS58" ]; then
    info "$FACTORY already points at $version. Nothing to do."
    exit 0
fi
info "$FACTORY: $current_version ($current) -> $version ($RELEASE_BS58)"

if [ "$current_version" != unknown ] && [ "$current_version" != v0 ]; then
    newer="$(printf '%s\n%s\n' "$current_version" "$version" | sort -V | tail -1)"
    if [ "$newer" = "$current_version" ] && [ "$rollback" = 0 ]; then
        die "$version is older than $current_version. Pass --rollback if that's intended."
    fi
fi

run_tx near contract call-function as-transaction "$FACTORY" set_global_code_hash \
    json-args "{\"code_hash_str\":\"$RELEASE_BS58\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
    sign-as "$FACTORY_OWNER" network-config "$NETWORK" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

after="$(view "$NETWORK" "$FACTORY" get_proxy_code_base58_hash | jq -r .)"
[ "$after" = "$RELEASE_BS58" ] || die "$FACTORY points at $after, not $RELEASE_BS58"
info "done: $FACTORY points at $version"
