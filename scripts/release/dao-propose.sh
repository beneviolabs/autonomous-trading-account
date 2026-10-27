#!/usr/bin/env bash
# Proposes a change to the prod factory to the DAO.
#
# Usage: scripts/release/dao-propose.sh <pointer <version> | add-key <public key>> --as <council account> [--hd-path <path>] [--print-only] [--dry-run]
#
#   pointer <version>   set_global_code_hash to that trading account release
#   add-key <key>       add_full_access_key, the temporary key for a factory code deploy
#
# Signs add_proposal as the council account, with a Ledger when --hd-path is given and from the
# keychain otherwise, attaching the 0.1 NEAR bond, then prints the proposal as stored. Voters vote
# however they like (Trezu or the near CLI). --print-only prints the method and arguments for
# submitting through Trezu instead.
# shellcheck source=../lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/../lib.sh"

args=() council="" hd_path="" print_only=0
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --print-only) print_only=1 ;;
        --as) council="${2:-}"; shift ;;
        --hd-path) hd_path="${2:-}"; shift ;;
        -*) usage ;;
        *) args+=("$1") ;;
    esac
    shift
done
[ "${#args[@]}" = 2 ] || usage

case "${args[0]}" in
    pointer)
        fetch_release "${args[1]}"
        global_exists mainnet "$RELEASE_BS58" \
            || die "${args[1]} isn't a global contract on mainnet yet: scripts/release/deploy-global.sh mainnet ${args[1]}"
        method=set_global_code_hash
        method_args="{\"code_hash_str\":\"$RELEASE_BS58\"}"
        commit="$(gh release view "trading-account/v${args[1]}" --repo "$GH_REPO" --json targetCommitish --jq .targetCommitish)"
        description="Point $PROD_FACTORY at trading account ${args[1]}: global code $RELEASE_BS58, released at https://github.com/$GH_REPO/releases/tag/trading-account/v${args[1]} (built from $commit)."
        ;;
    add-key)
        method=add_full_access_key
        method_args="{\"public_key\":\"${args[1]}\"}"
        description="Add a temporary full-access key ${args[1]} to $PROD_FACTORY, to deploy new factory code. It's deleted after the deploy."
        ;;
    *) usage ;;
esac

# The proposal's single function call on the factory. Voters' clients rebuild this from the
# stored proposal, so nothing here has to be shared with them.
args_b64="$(printf '%s' "$method_args" | base64 | tr -d '\n')"
kind="{\"FunctionCall\":{\"receiver_id\":\"$PROD_FACTORY\",\"actions\":[{\"method_name\":\"$method\",\"args\":\"$args_b64\",\"deposit\":\"0\",\"gas\":\"15000000000000\"}]}}"

cat >&2 <<EOF

DAO:          $DAO
Receiver:     $PROD_FACTORY
Method:       $method
Arguments:    $method_args
Base64:       $args_b64
Deposit, gas: 0, 15 Tgas
Description:  $description
EOF
[ "$print_only" = 1 ] && exit 0

[ -n "$council" ] || die "--as <council account> is required to submit"
signing_args "$hd_path" ""
proposal="$(jq -cn --arg d "$description" --argjson k "$kind" '{proposal: {description: $d, kind: $k}}')"
next_id="$(view mainnet "$DAO" get_last_proposal_id)"

run_tx near contract call-function as-transaction "$DAO" add_proposal json-args "$proposal" \
    prepaid-gas '30.0 Tgas' attached-deposit '0.1 NEAR' sign-as "$council" network-config mainnet "${SIGN[@]}" send
[ "$DRY_RUN" = 1 ] && exit 0

stored="$(view mainnet "$DAO" get_proposal "{\"id\":$next_id}")" \
    || die "couldn't read proposal $next_id back; check get_last_proposal_id"
[ "$(jq -r .proposer <<<"$stored")" = "$council" ] && [ "$(jq -cS .kind <<<"$stored")" = "$(jq -cS . <<<"$kind")" ] \
    || die "proposal $next_id isn't the one just submitted (another landed first?): $stored"
info "submitted proposal $next_id. It needs 3 of 4 council approvals."
