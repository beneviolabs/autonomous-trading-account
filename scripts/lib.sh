#!/usr/bin/env bash
# Shared helpers for the release and dev scripts. Source it; don't run it.
#
# Nothing here keeps state between runs: hashes come from the GitHub Releases or CI runs they
# were published in, wasm is downloaded into a temporary folder that's deleted on exit, and
# everything on chain is read from RPC at the moment it's needed.
#
# Needs: gh (signed in), jq, curl, python3, near (near-cli-rs).

# Callers read the globals set here (NETWORK, FACTORY, SIGN, ...).
# shellcheck disable=SC2034

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
GH_REPO="beneviolabs/autonomous-trading-account"

# The original trading account code. It predates GitHub Releases, so it's named here.
V0_HASH="6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f"

DAO="peerfolio.sputnik-dao.near"
PROD_FACTORY="auth.peerfolio.near"

# Prints the calling script's "# Usage:" line and exits.
usage() { grep -m1 '^# Usage:' "$0" | sed 's/^# //' >&2; exit 2; }
die() { echo "error: $*" >&2; exit 1; }
info() { echo "==> $*" >&2; }

# --- Targets --------------------------------------------------------------------------------

# Sets NETWORK, FACTORY and FACTORY_OWNER for a factory target: prod, dev or testnet.
factory_target() {
    case "$1" in
        prod)    NETWORK=mainnet FACTORY="$PROD_FACTORY"           FACTORY_OWNER="$DAO" ;;
        dev)     NETWORK=mainnet FACTORY=auth.dev.peerfolio.near   FACTORY_OWNER=dev.peerfolio.near ;;
        testnet) NETWORK=testnet FACTORY=auth.peerfolio.testnet    FACTORY_OWNER=peerfolio.peerfolio.testnet ;;
        *) die "unknown factory '$1': expected prod, dev or testnet" ;;
    esac
}

check_network() {
    case "$1" in
        mainnet|testnet) ;;
        *) die "unknown network '$1': expected mainnet or testnet" ;;
    esac
}

# The account that pays for global contracts on a network. Publishing by hash leaves the code
# owned by nobody, so the deployer only pays the storage; these team accounts are already in the
# keychain for releases, which keeps peerfolio.near's key out of it.
global_deployer() {
    check_network "$1"
    case "$1" in
        mainnet) echo dev.peerfolio.near ;;
        testnet) echo peerfolio.peerfolio.testnet ;;
    esac
}

# --- Confirmation ---------------------------------------------------------------------------

DRY_RUN=0

# Prints a command, asks before running it, and runs it. With --dry-run it only prints.
# Declining returns 1, which ends a script (set -e) but lets the dev session carry on.
run_tx() {
    echo >&2
    printf '  %q' "$@" >&2
    echo >&2
    echo >&2
    if [ "$DRY_RUN" = 1 ]; then
        info "dry run: not sent"
        return 0
    fi
    local answer
    read -r -p "Send? [y/N] " answer
    if [ "$answer" != y ] && [ "$answer" != Y ]; then
        echo "not sent" >&2
        return 1
    fi
    "$@"
}

# --- RPC ------------------------------------------------------------------------------------

rpc_url() {
    case "$1" in
        mainnet) echo "${NEAR_MAINNET_RPC:-https://free.rpc.fastnear.com}" ;;
        testnet) echo "${NEAR_TESTNET_RPC:-https://rpc.testnet.fastnear.com}" ;;
        *) die "unknown network '$1'" ;;
    esac
}

# rpc_query <network> <params JSON>: the query's result object, or the RPC's error on stderr.
rpc_query() {
    local response
    response="$(curl -sS --fail "$(rpc_url "$1")" -H 'content-type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"query\",\"params\":$2}")" \
        || die "RPC request to $1 failed"
    if [ "$(jq -r '.error // empty | .cause.name // .name' <<<"$response")" != "" ]; then
        jq -c .error <<<"$response" >&2
        return 1
    fi
    jq -c .result <<<"$response"
}

# view <network> <account> <method> [args JSON]: the view call's return value, as JSON.
view() {
    local args="${4:-}" args_b64
    [ -n "$args" ] || args='{}'
    args_b64="$(printf '%s' "$args" | base64 | tr -d '\n')"
    local result
    result="$(rpc_query "$1" "{\"request_type\":\"call_function\",\"finality\":\"final\",\"account_id\":\"$2\",\"method_name\":\"$3\",\"args_base64\":\"$args_b64\"}")" \
        || return 1
    # A call that fails in the contract comes back as a result with an error string.
    if jq -e 'has("error")' <<<"$result" >/dev/null; then
        echo "$2.$3: $(jq -r .error <<<"$result")" >&2
        return 1
    fi
    jq -c '.result | implode | fromjson' <<<"$result"
}

# agents_of <network> <account>: a trading account's agents as a sorted JSON array. v0 calls them
# authorized users.
agents_of() {
    { view "$1" "$2" get_agents 2>/dev/null || view "$1" "$2" get_authorized_users; } | jq -cS 'sort'
}

# view_account <network> <account>: code_hash, global_contract_hash and the rest.
view_account() {
    rpc_query "$1" "{\"request_type\":\"view_account\",\"finality\":\"final\",\"account_id\":\"$2\"}"
}

# access_keys <network> <account>: one public key per line.
access_keys() {
    rpc_query "$1" "{\"request_type\":\"view_access_key_list\",\"finality\":\"final\",\"account_id\":\"$2\"}" \
        | jq -r '.keys[].public_key'
}

# view_state <network> <account>: the contract's storage, [{key, value}] base64, sorted by key.
view_state() {
    rpc_query "$1" "{\"request_type\":\"view_state\",\"finality\":\"final\",\"account_id\":\"$2\",\"prefix_base64\":\"\"}" \
        | jq -c '[.values[] | {key, value}] | sort_by(.key)'
}

# global_exists <network> <bs58 hash>
global_exists() {
    rpc_query "$1" "{\"request_type\":\"view_global_contract_code\",\"finality\":\"final\",\"code_hash\":\"$2\"}" \
        >/dev/null 2>&1
}

# --- Hashes ---------------------------------------------------------------------------------

sha256_hex() {
    if command -v sha256sum >/dev/null; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# The bs58 form is what deploy-as-global prints, set_global_code_hash takes and RPC returns.
hex_to_bs58() {
    python3 -c '
import sys
digest = bytes.fromhex(sys.argv[1])
n = int.from_bytes(digest, "big")
alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
out = ""
while n:
    n, r = divmod(n, 58)
    out = alphabet[r] + out
print("1" * (len(digest) - len(digest.lstrip(b"\0"))) + out)
' "$1"
}

# --- Releases -------------------------------------------------------------------------------

# Sets WORK_DIR to a temporary folder for downloads, deleted when the script exits. Call it
# directly, not in $(...), or the folder is deleted as soon as the subshell ends.
WORK_DIR=""
work_dir() {
    if [ -z "$WORK_DIR" ]; then
        WORK_DIR="$(mktemp -d)"
        trap 'rm -rf "$WORK_DIR"' EXIT
    fi
}

# Every trading account release, one "<version> <bs58 hash>" per line, oldest first, read from
# the GitHub Releases' notes. v0 comes first.
release_versions() {
    echo "v0 $V0_HASH"
    gh api "repos/$GH_REPO/releases" --paginate \
        --jq '.[] | select(.tag_name | startswith("trading-account/v")) | [.tag_name, .body] | @tsv' \
        | while IFS=$'\t' read -r tag body; do
            local bs58
            bs58="$(printf '%b' "$body" | sed -nE 's/^SHA-256 \(bs58\): `([^`]+)`.*/\1/p')"
            if [ -n "$bs58" ]; then echo "${tag#trading-account/v} $bs58"; fi
        done | sort -V
}

# version_of_hash <bs58 hash>: its release version, or "unknown".
version_of_hash() {
    local version
    version="$(release_versions | awk -v h="$1" '$2 == h { print $1 }')"
    echo "${version:-unknown}"
}

# fetch_release <version>: downloads that trading account Release and checks its hash.
# Sets RELEASE_WASM and RELEASE_BS58.
fetch_release() {
    local tag="trading-account/v$1" dir notes hex
    work_dir
    dir="$WORK_DIR/release-$1"
    notes="$(gh release view "$tag" --repo "$GH_REPO" --json body --jq .body)" \
        || die "no GitHub Release $tag"
    hex="$(sed -nE 's/^SHA-256 \(hex\): `([^`]+)`.*/\1/p' <<<"$notes")"
    RELEASE_BS58="$(sed -nE 's/^SHA-256 \(bs58\): `([^`]+)`.*/\1/p' <<<"$notes")"
    [[ -n "$hex" && -n "$RELEASE_BS58" ]] || die "$tag's notes have no hashes"
    gh release download "$tag" --repo "$GH_REPO" -p trading_account.wasm -D "$dir" >/dev/null
    RELEASE_WASM="$dir/trading_account.wasm"
    [ "$(sha256_hex "$RELEASE_WASM")" = "$hex" ] || die "$tag's wasm doesn't match the hash in its notes"
    [ "$(hex_to_bs58 "$hex")" = "$RELEASE_BS58" ] || die "$tag's hex and bs58 hashes disagree"
    info "trading account $1: $RELEASE_BS58 (checked)"
}

# sandbox_test <wasm>: runs the trading account's sandbox tests against that wasm instead of this
# checkout's build. The tests fetch a live 1Click quote, so they need the network.
sandbox_test() {
    info "running the sandbox tests against $1"
    work_dir
    (cd "$REPO_ROOT/contracts" && TRADING_ACCOUNT_WASM="$1" \
        cargo test -p trading-account --features integration-tests --lib integration_tests) >"$WORK_DIR/sandbox-test.log" 2>&1 \
        || { grep -E '^test .*FAILED|panicked|^test result' "$WORK_DIR/sandbox-test.log" >&2; die "sandbox tests failed against $1 (log: $WORK_DIR/sandbox-test.log)"; }
    info "sandbox tests passed: $(grep '^test result' "$WORK_DIR/sandbox-test.log" | tail -1)"
}

# fetch_ci_factory <commit> <require main: 0|1>: downloads the factory wasm from that commit's
# successful CI run and checks the run printed its hash. Sets FACTORY_WASM and FACTORY_BS58.
fetch_ci_factory() {
    local commit run dir hex
    commit="$(git -C "$REPO_ROOT" rev-parse --verify "$1^{commit}" 2>/dev/null)" \
        || die "unknown commit $1 (git fetch first?)"
    if [ "$2" = 1 ]; then
        git -C "$REPO_ROOT" fetch -q origin main
        git -C "$REPO_ROOT" merge-base --is-ancestor "$commit" origin/main \
            || die "$commit isn't on main; mainnet deploys come from main"
    fi
    run="$(gh run list --repo "$GH_REPO" --workflow contracts.yml --commit "$commit" \
        --json databaseId,event,conclusion \
        --jq '[.[] | select(.conclusion == "success" and (.event == "push" or .event == "workflow_dispatch"))][0].databaseId')"
    [[ -n "$run" && "$run" != null ]] || die "no successful push or manual CI run for $commit. Start one: gh workflow run contracts.yml --ref <branch>"
    work_dir
    dir="$WORK_DIR/ci-$commit"
    gh run download "$run" --repo "$GH_REPO" -n contract-wasm -D "$dir" \
        || die "couldn't download run $run's wasm; artifacts expire after 90 days"
    FACTORY_WASM="$dir/trading_account_factory/trading_account_factory.wasm"
    hex="$(sha256_hex "$FACTORY_WASM")"
    gh run view "$run" --repo "$GH_REPO" --log | grep -F "SHA-256 checksum hex : $hex" >/dev/null \
        || die "run $run's log doesn't show the hash of the wasm it uploaded"
    FACTORY_BS58="$(hex_to_bs58 "$hex")"
    info "factory at ${commit:0:7} (run $run): $FACTORY_BS58 (checked)"
}

# --- Dev ------------------------------------------------------------------------------------

# Builds scripts/dev/restore-state, the helper restore.sh and reset-to-v0.sh deploy for one
# transaction. Sets RESTORE_HELPER to its wasm.
build_restore_helper() {
    (cd "$REPO_ROOT/scripts/dev/restore-state" && cargo near build non-reproducible-wasm --no-abi) >/dev/null 2>&1 \
        || die "couldn't build scripts/dev/restore-state (run cargo near build non-reproducible-wasm in it to see why)"
    RESTORE_HELPER="$REPO_ROOT/scripts/dev/restore-state/target/near/restore_state.wasm"
}

# --- Signing --------------------------------------------------------------------------------

# Sets SIGN to the near CLI signing arguments: a Ledger with --hd-path, a key file with
# --key-file, otherwise the keychain.
signing_args() {
    local hd_path="$1" key_file="$2"
    if [ -n "$hd_path" ]; then
        SIGN=(sign-with-ledger --seed-phrase-hd-path "$hd_path")
    elif [ -n "$key_file" ]; then
        SIGN=(sign-with-access-key-file "$key_file")
    else
        SIGN=(sign-with-keychain)
    fi
}
