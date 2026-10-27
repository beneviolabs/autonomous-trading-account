#!/usr/bin/env bash
# Shows what the factories run and point at. Read-only.
#
# Usage: scripts/status.sh [mainnet|testnet]
#
# For each factory on the network (mainnet: prod and dev; testnet: testnet): its code hash, the
# trading account release it points new accounts at, and its access keys. Exits non-zero if the
# prod factory has any key, or a factory points at code that isn't a release. Run it before and
# after a release.
# shellcheck source=lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

network="${1:-mainnet}"
case "$network" in
    mainnet) targets=(prod dev) ;;
    testnet) targets=(testnet) ;;
    *) usage ;;
esac

versions="$(release_versions)"
problems=0
for target in "${targets[@]}"; do
    factory_target "$target"
    code="$(view_account "$NETWORK" "$FACTORY" | jq -r .code_hash)"
    pointer="$(view "$NETWORK" "$FACTORY" get_proxy_code_base58_hash | jq -r .)"
    version="$(awk -v h="$pointer" '$2 == h { print $1 }' <<<"$versions")"
    keys="$(access_keys "$NETWORK" "$FACTORY")"

    echo "$FACTORY ($target)"
    echo "  owner          $(view "$NETWORK" "$FACTORY" get_owner_id | jq -r .)"
    echo "  factory code   $code"
    echo "  points at      ${version:-UNKNOWN} ($pointer)"
    if [ -z "$version" ]; then
        echo "  !! $pointer isn't the hash of any trading account release"
        problems=1
    fi
    if [ -z "$keys" ]; then
        echo "  keys           none"
    else
        while read -r key; do echo "  key            $key"; done <<<"$keys"
        if [ "$target" = prod ]; then
            echo "  !! the prod factory should have no keys. Delete a release's temporary key once its deploy is done."
            problems=1
        fi
    fi
    echo
done

if [ "$network" = mainnet ]; then
    factory_target prod; prod_code="$(view_account mainnet "$FACTORY" | jq -r .code_hash)"
    factory_target dev; dev_code="$(view_account mainnet "$FACTORY" | jq -r .code_hash)"
    [ "$prod_code" = "$dev_code" ] || echo "note: prod and dev run different factory code (expected mid-release)"
fi
exit "$problems"
