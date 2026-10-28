#!/usr/bin/env bash
# Deploys factory code from a CI build to an existing factory, keeping its stored state.
#
# Usage: scripts/release/deploy-factory.sh <testnet|dev|prod> <commit> [--hd-path <path> | --key-file <file>] [--dry-run]
#
# Takes the wasm from the commit's successful push or manual CI run: any branch for testnet and
# dev, main only for prod. Deploys without an init call, then checks the code hash and that the
# owner, signer and trading account pointer haven't changed.
#
# testnet and dev sign from the keychain as the factory account. prod has no keys of its own:
# first get your public key added through the DAO (dao-propose.sh add-key), then sign with it
# here, with --hd-path for a Ledger or --key-file. After a successful prod deploy, the script
# deletes that key. If the deploy fails, the key stays, so you can retry without another vote.
# shellcheck source=../lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/../lib.sh"

args=() hd_path="" key_file=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --hd-path) hd_path="${2:-}"; shift ;;
        --key-file) key_file="${2:-}"; shift ;;
        -*) usage ;;
        *) args+=("$1") ;;
    esac
    shift
done
[ "${#args[@]}" = 2 ] || usage
target="${args[0]}"
factory_target "$target"
signing_args "$hd_path" "$key_file"

require_main=0
if [ "$target" = prod ]; then
    require_main=1
    [ -n "$hd_path$key_file" ] || die "prod needs --hd-path or --key-file for the key the DAO added"
    keys="$(access_keys mainnet "$FACTORY")"
    [ "$(grep -c . <<<"$keys")" = 1 ] \
        || die "$FACTORY should have exactly one key, the one the DAO added. It has: ${keys:-none}"
    temp_key="$keys"
    info "signing with $temp_key, the key on $FACTORY"
fi
fetch_ci_factory "${args[1]}" "$require_main"

state() {
    echo "owner $(view "$NETWORK" "$FACTORY" get_owner_id)"
    echo "signer $(view "$NETWORK" "$FACTORY" get_signer_contract)"
    echo "pointer $(view "$NETWORK" "$FACTORY" get_proxy_code_base58_hash)"
}
before="$(state)"
current="$(view_account "$NETWORK" "$FACTORY" | jq -r .code_hash)"
info "$FACTORY code: $current -> $FACTORY_BS58"
if [ "$current" = "$FACTORY_BS58" ]; then
    info "already deployed"
else
    run_tx near contract deploy "$FACTORY" use-file "$FACTORY_WASM" without-init-call \
        network-config "$NETWORK" "${SIGN[@]}" send
    [ "$DRY_RUN" = 1 ] && exit 0
fi

after_code="$(view_account "$NETWORK" "$FACTORY" | jq -r .code_hash)"
[ "$after_code" = "$FACTORY_BS58" ] || die "$FACTORY runs $after_code, not $FACTORY_BS58"
after="$(state)"
[ "$after" = "$before" ] || die "$FACTORY's state reads differently after the deploy:
before:
$before
after:
$after"
info "done: $FACTORY runs $FACTORY_BS58, and its owner, signer and pointer are unchanged"

if [ "$target" = prod ]; then
    info "deleting the temporary key $temp_key. The factory should have no keys."
    run_tx near account delete-keys "$FACTORY" public-keys "$temp_key" \
        network-config mainnet "${SIGN[@]}" send
    [ "$DRY_RUN" = 1 ] && exit 0
    [ -z "$(access_keys mainnet "$FACTORY")" ] || die "$FACTORY still has keys. Delete them before you finish."
    info "done: $FACTORY has no keys"
fi
