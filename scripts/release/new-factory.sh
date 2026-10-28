#!/usr/bin/env bash
# Creates and initializes a new factory account from a CI build.
#
# Usage: scripts/release/new-factory.sh <mainnet|testnet> <factory account> <owner> <trading account version> <commit> [--dry-run]
#
# Only for a new factory account, for example when the factory's stored state changes. To update
# an existing factory's code, use deploy-factory.sh.
#
# - The factory account is created by its parent account (auth.peerfolio.near by peerfolio.near)
#   with 4 NEAR, signed from the keychain, and gets a new keychain key.
# - The wasm comes from the commit's successful push or manual CI run (main only on mainnet).
# - new() gets the owner, the network and the trading account version's global hash, which must
#   already be deployed on that network. "v0" is the original code.
# - Afterwards, a factory owned by a DAO should have no keys: the script offers to delete the one
#   it created.
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
[ "${#args[@]}" = 5 ] || usage
network="${args[0]}" factory="${args[1]}" owner="${args[2]}" version="${args[3]}" commit="${args[4]}"
global_deployer "$network" >/dev/null
root="${factory#*.}"
[ "$root" != "$factory" ] || die "$factory has no parent account to create it"
[ "${#factory}" -le 30 ] || die "$factory is over 30 characters, so trading account IDs wouldn't fit NEAR's 64"
# An account with no code is a run that stopped after creating it: carry on from the deploy.
existing="$(view_account "$network" "$factory" 2>/dev/null | jq -r .code_hash)" || existing=""
if [ -n "$existing" ] && [ "$existing" != 11111111111111111111111111111111 ]; then
    die "$factory already runs code. To update it, use deploy-factory.sh."
fi

if [ "$version" = v0 ]; then
    hash="$V0_HASH"
else
    fetch_release "$version"
    hash="$RELEASE_BS58"
fi
global_exists "$network" "$hash" || die "$version ($hash) isn't a global contract on $network yet"
require_main=0
[ "$network" = mainnet ] && require_main=1
fetch_ci_factory "$commit" "$require_main"

if [ -z "$existing" ]; then
    info "creating $factory from $root with 4 NEAR"
    run_tx near account create-account fund-myself "$factory" '4 NEAR' autogenerate-new-keypair save-to-keychain \
        sign-as "$root" network-config "$network" sign-with-keychain send
else
    info "$factory exists with no code, so it was created by an earlier run. Deploying to it."
fi

init_args="{\"owner_id\":\"$owner\",\"network\":\"$network\",\"global_proxy_base58_hash\":\"$hash\"}"
info "deploying the factory and calling new($init_args)"
run_tx near contract deploy "$factory" use-file "$FACTORY_WASM" with-init-call new json-args "$init_args" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config "$network" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

[ "$(view_account "$network" "$factory" | jq -r .code_hash)" = "$FACTORY_BS58" ] || die "$factory doesn't run $FACTORY_BS58"
[ "$(view "$network" "$factory" get_owner_id | jq -r .)" = "$owner" ] || die "$factory's owner isn't $owner"
[ "$(view "$network" "$factory" get_proxy_code_base58_hash | jq -r .)" = "$hash" ] || die "$factory doesn't point at $hash"
info "done: $factory runs $FACTORY_BS58, owned by $owner, pointing at $version"

key="$(access_keys "$network" "$factory")"
info "$factory has key $key. If its owner is a DAO, delete it so only the DAO controls the factory."
run_tx near account delete-keys "$factory" public-keys "$key" network-config "$network" sign-with-keychain send
info "Then point Peerfolio Backend at it: AUTH_CREATOR and VITE_AUTH_CREATOR_* (local .env, Render, Cloudflare)."
