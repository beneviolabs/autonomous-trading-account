#!/usr/bin/env bash
# Prints a restore point for a trading account as JSON. Read-only.
#
# Usage: scripts/dev/snapshot.sh <trading account> [mainnet|testnet] > snapshot.json
#
# Records the contract's storage, its code (the global hash, or the wasm itself when the code is
# deployed on the account), its access keys and its NEAR balance. scripts/dev/restore.sh puts the
# storage and code back from it.
# shellcheck source=../lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/../lib.sh"

[[ $# -ge 1 && $# -le 2 ]] || usage
account="$1" network="${2:-mainnet}"
global_deployer "$network" >/dev/null

view="$(view_account "$network" "$account")" || die "$account doesn't exist on $network"
storage="$(view_state "$network" "$account")"
global="$(jq -r '.global_contract_hash // empty' <<<"$view")"
if [ -n "$global" ]; then
    code="$(jq -cn --arg h "$global" '{global_contract_hash: $h}')"
else
    code_base64="$(rpc_query "$network" "{\"request_type\":\"view_code\",\"finality\":\"final\",\"account_id\":\"$account\"}" | jq -r .code_base64)"
    code="$(jq -cn --arg h "$(jq -r .code_hash <<<"$view")" --arg c "$code_base64" '{code_hash: $h, code_base64: $c}')"
fi

jq -n \
    --arg account "$account" --arg network "$network" --arg taken_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --argjson code "$code" \
    --argjson storage "$storage" \
    --argjson keys "$(access_keys "$network" "$account" | jq -R . | jq -sc 'sort')" \
    --arg balance "$(jq -r .amount <<<"$view")" \
    '{account: $account, network: $network, taken_at: $taken_at, code: $code, storage: $storage, keys: $keys, balance: $balance}'
info "snapshot of $account: $(jq -r 'if .global_contract_hash then "global " + .global_contract_hash else "local code " + .code_hash end' <<<"$code"), $(jq length <<<"$storage") storage entries"
