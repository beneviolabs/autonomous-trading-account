#!/usr/bin/env bash
# Puts a trading account's storage and code back to a snapshot from scripts/dev/snapshot.sh.
#
# Usage: scripts/dev/restore.sh <snapshot.json> [--dry-run]
#
# One transaction, signed from the keychain as the trading account: deploy the restore helper
# (scripts/dev/restore-state), call its restore to write back every snapshot entry and remove
# the ones written since, then put back the snapshot's code. The actions share a receipt, so if
# any fails nothing changes, and the helper never stays on the account. Then checks storage and
# code match.
#
# Access keys and NEAR aren't contract state, so it only reports how they differ from the
# snapshot, with the commands to put them back.
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
[ "${#args[@]}" = 1 ] || usage
snapshot="${args[0]}"
[ -f "$snapshot" ] || die "no snapshot at $snapshot"
build_restore_helper
account="$(jq -r .account "$snapshot")" network="$(jq -r .network "$snapshot")"

global="$(jq -r '.code.global_contract_hash // empty' "$snapshot")"
if [ -n "$global" ]; then
    code_back=(use-global-contract use-global-hash "$global" without-init-call)
    code_label="global $global"
else
    work_dir
    wasm="$WORK_DIR/snapshot-code.wasm"
    jq -r .code.code_base64 "$snapshot" | base64 -d >"$wasm"
    code_back=(deploy-contract use-file "$wasm" without-init-call)
    code_label="its own code $(jq -r .code.code_hash "$snapshot")"
fi

now="$(view_state "$network" "$account")"
restore_args="$(jq -c --argjson now "$now" \
    '{entries: (.storage | map([.key, .value])), remove: ([$now[].key] - [.storage[].key])}' "$snapshot")"
info "restoring $account to $(jq -r .taken_at "$snapshot"): writes $(jq '.entries | length' <<<"$restore_args") entries, removes $(jq '.remove | length' <<<"$restore_args"), code back to $code_label"

run_tx near transaction construct-transaction "$account" "$account" \
    add-action deploy-contract use-file "$RESTORE_HELPER" with-init-call restore \
    json-args "$restore_args" prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    add-action "${code_back[@]}" \
    skip network-config "$network" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

[ "$(view_state "$network" "$account")" = "$(jq -c .storage "$snapshot")" ] || die "$account's storage doesn't match the snapshot"
view="$(view_account "$network" "$account")"
if [ -n "$global" ]; then
    [ "$(jq -r '.global_contract_hash // empty' <<<"$view")" = "$global" ] || die "$account isn't back on global $global"
else
    [ "$(jq -r .code_hash <<<"$view")" = "$(jq -r .code.code_hash "$snapshot")" ] || die "$account's code isn't the snapshot's"
fi
info "done: $account's storage and code match the snapshot"

keys_now="$(access_keys "$network" "$account" | jq -R . | jq -sc 'sort')"
added="$(jq -r --argjson now "$keys_now" '$now - .keys | .[]' "$snapshot")"
removed="$(jq -r --argjson now "$keys_now" '.keys - $now | .[]' "$snapshot")"
for key in $added; do
    info "key added since the snapshot: $key. To delete it: near account delete-keys $account public-keys $key network-config $network sign-with-keychain send"
done
for key in $removed; do info "key deleted since the snapshot: $key"; done

diff="$(python3 -c 'import sys; print((int(sys.argv[1]) - int(sys.argv[2])) / 1e24)' "$(jq -r .amount <<<"$view")" "$(jq -r .balance "$snapshot")")"
info "NEAR balance: $diff NEAR since the snapshot. If you topped it up, send that back yourself."
