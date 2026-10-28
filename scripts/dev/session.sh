#!/usr/bin/env bash
# Interactive dev session on one trading account: snapshot it, flash a dev build of this
# checkout, try things from a menu, then restore it.
#
# Usage: scripts/dev/session.sh <trading account> [mainnet|testnet]
#
# The snapshot goes to a temporary file whose path is printed first. Quitting without restoring
# prints the command to restore later. Every transaction is shown and asked about before it's
# sent. Calls are signed from the keychain: the trading account's key to flash and restore, the
# owner's (or an agent's) for the rest.
# shellcheck source=../lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/../lib.sh"

[ $# -ge 1 ] && [ $# -le 2 ] || usage
account="$1" network="${2:-mainnet}"
global_deployer "$network" >/dev/null
DEV="$REPO_ROOT/scripts/dev"

WNEAR=nep141:wrap.near
USDC=nep141:17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1

# ask <prompt> <default>: reads a value, or the default on an empty line.
ask() {
    local answer
    read -r -p "$1 [$2]: " answer
    echo "${answer:-$2}"
}

owner="$(view "$network" "$account" get_owner_id | jq -r .)" || die "$account isn't a trading account on $network"

snapshots="${TMPDIR:-/tmp}"
snapshots="${snapshots%/}/flash-dev"
mkdir -p "$snapshots"
snapshot="$snapshots/$account-$(date +%Y%m%dT%H%M%S).json"
"$DEV/snapshot.sh" "$account" "$network" >"$snapshot"
info "snapshot saved: $snapshot"
restored=0
trap '[ "$restored" = 1 ] || echo "Not restored. To restore later: scripts/dev/restore.sh $snapshot" >&2' EXIT

"$DEV/flash-dev.sh" "$account" "$network" || info "not flashed; the menu still works on the code it runs"

show_state() {
    echo "  version   $(view "$network" "$account" contract_version 2>/dev/null || echo v0)"
    echo "  code      $(view_account "$network" "$account" | jq -r '.global_contract_hash // .code_hash')"
    echo "  owner     $owner"
    echo "  agents    $(view "$network" "$account" get_agents 2>/dev/null || view "$network" "$account" get_authorized_users)"
    echo "  NEAR      $(view_account "$network" "$account" | jq -r '(.amount | tonumber) / 1e24')"
    local tokens
    tokens="$(view "$network" intents.near mt_tokens_for_owner "{\"account_id\":\"$account\"}" | jq -c '[.[].token_id]')"
    if [ "$tokens" = "[]" ]; then
        echo "  intents   nothing"
    else
        jq -r --argjson b "$(view "$network" intents.near mt_batch_balance_of "{\"account_id\":\"$account\",\"token_ids\":$tokens}")" \
            'to_entries[] | "  intents   \(.value)  \($b[.key])"' <<<"$tokens"
    fi
}

# 1Click quote -> swap(quote_payload, quote_signature), then follow the deposit to a final status.
swap() {
    local from to amount recipient signer response payload signature deposit status
    from="$(ask "From token" "$WNEAR")"
    to="$(ask "To token" "$USDC")"
    amount="$(ask "Amount in, smallest units" 20000000000000000000000)"
    recipient="$(ask "Recipient: the trading account, or the owner to liquidate" "$account")"
    signer="$(ask "Sign as (owner or agent)" "$owner")"
    response="$(curl -sS https://1click.chaindefuser.com/v0/quote -H 'content-type: application/json' -d "$(jq -cn \
        --arg ta "$account" --arg from "$from" --arg to "$to" --arg amount "$amount" --arg recipient "$recipient" \
        --arg deadline "$(date -u -v+15M +%Y-%m-%dT%H:%M:%S.000Z 2>/dev/null || date -u -d '+15 min' +%Y-%m-%dT%H:%M:%S.000Z)" \
        '{dry: false, swapType: "EXACT_INPUT", slippageTolerance: 100, originAsset: $from, destinationAsset: $to,
          depositType: "INTENTS", amount: $amount, refundTo: $ta, refundType: "INTENTS", recipient: $recipient,
          recipientType: "INTENTS", deadline: $deadline, referral: "peerfolio.near"}')")"
    deposit="$(jq -r '.quote.depositAddress // empty' <<<"$response")"
    [ -n "$deposit" ] || { echo "No quote: $response" >&2; return 1; }
    echo "  quote: $(jq -r '.quote | "\(.amountInFormatted) in -> \(.amountOutFormatted) out"' <<<"$response")"
    # The exact JSON 1Click signed, which swap verifies byte for byte.
    payload="$(python3 -I "$DEV/quote_payload.py" <<<"$response")"
    signature="$(jq -r .signature <<<"$response")"
    run_tx near contract call-function as-transaction "$account" swap \
        json-args "$(jq -cn --arg p "$payload" --arg s "$signature" '{quote_payload: $p, quote_signature: $s}')" \
        prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$signer" network-config "$network" sign-with-keychain send \
        || return 1
    for _ in $(seq 1 24); do
        status="$(curl -sS "https://1click.chaindefuser.com/v0/status?depositAddress=$deposit" | jq -r .status)"
        echo "  1Click status: $status"
        case "$status" in SUCCESS|REFUNDED|FAILED) break ;; esac
        sleep 5
    done
}

transfer_to_owner() {
    local token amount signer
    token="$(ask "Token" "$WNEAR")"
    amount="$(ask "Amount, smallest units" 1000000000000000000000)"
    signer="$(ask "Sign as (owner or agent)" "$owner")"
    run_tx near contract call-function as-transaction "$account" transfer_to_owner \
        json-args "$(jq -cn --arg t "$token" --arg a "$amount" '{tokens: {($t): $a}}')" \
        prepaid-gas '50.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$signer" network-config "$network" sign-with-keychain send
}

agent() {
    local which agent_id
    which="$(ask "add or remove" add)"
    agent_id="$(ask "Agent account" bot.dev.peerfolio.near)"
    run_tx near contract call-function as-transaction "$account" "${which}_agent" \
        json-args "{\"account_id\":\"$agent_id\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
        sign-as "$owner" network-config "$network" sign-with-keychain send
}

while true; do
    cat <<EOF

What next?
  1) show state
  2) swap with a live 1Click quote   [moves funds]
  3) transfer_to_owner               [moves funds]
  4) add or remove an agent
  5) re-flash (rebuild after code changes)
  r) restore to the snapshot and quit
  q) quit without restoring
EOF
    read -r -p "> " choice || choice=q
    case "$choice" in
        1) show_state || true ;;
        2) swap || true ;;
        3) transfer_to_owner || true ;;
        4) agent || true ;;
        5) "$DEV/flash-dev.sh" "$account" "$network" || true ;;
        r) "$DEV/restore.sh" "$snapshot" && restored=1 && exit 0 ;;
        q) exit 0 ;;
        *) echo "?" ;;
    esac
done
