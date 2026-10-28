#!/usr/bin/env bash
# Puts a flashed or upgraded trading account back on the v0 global code, with v0 state.
#
# Usage: scripts/dev/reset-to-v0.sh <trading account> [mainnet|testnet] [--dry-run]
#
# One transaction, signed from the keychain as the trading account: deploy the restore helper
# (tools/restore-state), call its reset_to_v0 to rewrite v1 state into v0's layout, then switch to
# the v0 global code. That also removes the account's own code. The actions share a receipt, so
# if any fails nothing changes, and the helper never stays on the account. Then checks the owner
# and agents read back on v0.
#
# For going back to exactly where a dev session started, use restore.sh with its snapshot instead.
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
[ "${#args[@]}" -ge 1 ] && [ "${#args[@]}" -le 2 ] || usage
account="${args[0]}" network="${args[1]:-mainnet}"
case "$network" in
    mainnet) signer_id=v1.signer ;;
    testnet) signer_id=v1.signer-prod.testnet ;;
    *) usage ;;
esac

[ "$(view_account "$network" "$account" | jq -r '.global_contract_hash // empty')" != "$V0_HASH" ] \
    || die "$account already runs v0"
owner="$(view "$network" "$account" get_owner_id | jq -r .)" || die "$account isn't a trading account on $network"
agents="$(view "$network" "$account" get_agents | jq -cS 'sort')" \
    || die "couldn't read $account's agents: is it on v1 state?"

(cd "$REPO_ROOT/tools/restore-state" && cargo near build non-reproducible-wasm --no-abi) >/dev/null 2>&1 \
    || die "couldn't build tools/restore-state (run cargo near build non-reproducible-wasm in it to see why)"
helper="$REPO_ROOT/tools/restore-state/target/near/restore_state.wasm"

info "resetting $account to v0 ($V0_HASH): owner $owner, agents $agents, signer $signer_id"
run_tx near transaction construct-transaction "$account" "$account" \
    add-action deploy-contract use-file "$helper" with-init-call reset_to_v0 \
    json-args "{\"signer_id\":\"$signer_id\"}" prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    add-action use-global-contract use-global-hash "$V0_HASH" without-init-call \
    skip network-config "$network" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

view="$(view_account "$network" "$account")"
[ "$(jq -r '.global_contract_hash // empty' <<<"$view")" = "$V0_HASH" ] || die "$account isn't on the v0 global"
[ "$(jq -r .code_hash <<<"$view")" = 11111111111111111111111111111111 ] || die "$account still has code of its own"
[ "$(view "$network" "$account" get_owner_id | jq -r .)" = "$owner" ] || die "$account's owner doesn't read back on v0"
[ "$(view "$network" "$account" get_authorized_users | jq -cS 'sort')" = "$agents" ] \
    || die "$account's agents don't read back on v0"
[ "$(view "$network" "$account" get_signer_id | jq -r .)" = "$signer_id" ] || die "$account's signer isn't $signer_id"
info "done: $account runs v0 with owner $owner and agents $agents"
