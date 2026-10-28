#!/usr/bin/env bash
# Deploys a dev build of the current checkout straight onto one trading account, for trying
# unreleased code. It replaces whatever code the account ran, and the account then runs code no
# release has, so use it only on accounts you mean to.
#
# Usage: scripts/dev/flash-dev.sh <trading account> [mainnet|testnet] [--dry-run]
#
# Builds the trading account without ONE_CLICK_TEST_KEY and refuses if the wasm still trusts the
# test key. Deploys it with migrate, signed from the keychain as the trading account (its key
# must be there), then checks the code hash, version, owner and agents.
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
[[ "${#args[@]}" -ge 1 && "${#args[@]}" -le 2 ]] || usage
account="${args[0]}" network="${args[1]:-mainnet}"
global_deployer "$network" >/dev/null

# Owner and agents, read before and after.
read_state() {
    echo "owner $(view "$network" "$account" get_owner_id)"
    echo "agents $(agents_of "$network" "$account")"
}
before="$(read_state)" || die "couldn't read $account's owner and agents on $network"

info "building the trading account"
work_dir
env -u ONE_CLICK_TEST_KEY "$REPO_ROOT/scripts/build-wasm.sh" "$REPO_ROOT/contracts/trading-account" >"$WORK_DIR/build.log" 2>&1 \
    || { tail -20 "$WORK_DIR/build.log" >&2; die "build failed"; }
wasm="$REPO_ROOT/contracts/target/near/trading_account/trading_account.wasm"
if LC_ALL=C grep -aqF "$ONE_CLICK_TEST_PUBLIC_KEY" "$wasm"; then
    die "$wasm trusts the sandbox tests' quote key. Not deploying it."
fi
bs58="$(hex_to_bs58 "$(sha256_hex "$wasm")")"
info "flashing $account on $network with a dev build of $(git -C "$REPO_ROOT" rev-parse --short HEAD) ($bs58)"
[ -z "$(git -C "$REPO_ROOT" status --porcelain -- contracts)" ] || info "note: contracts/ has uncommitted changes, and they're in this build"

run_tx near contract deploy "$account" use-file "$wasm" with-init-call migrate json-args '{}' \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config "$network" sign-with-keychain send
[ "$DRY_RUN" = 1 ] && exit 0

[ "$(view_account "$network" "$account" | jq -r .code_hash)" = "$bs58" ] || die "$account doesn't run $bs58"
after="$(read_state)"
[ "$after" = "$before" ] || die "$account's owner or agents changed:
before:
$before
after:
$after"
info "done: $account runs $bs58, version $(view "$network" "$account" contract_version), with the same owner and agents"
