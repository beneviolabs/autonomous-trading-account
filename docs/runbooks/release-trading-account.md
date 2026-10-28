# Runbook: release trading account code

Publishes new trading account code and points the factories at it: dev first, prod through the DAO. Existing accounts change only when their owners [upgrade](../trading-account.md#upgrading-a-trading-account). Background: [Releasing](../releasing.md).

Every script prints the transaction and asks before sending it. Add `--dry-run` to only print. Run `scripts/release/<script>.sh` with no arguments for its usage.

## Before you start

- [ ] A PR that bumps `CONTRACT_VERSION` is merged to `main` ([Versions](../releasing.md#versions)).
- [ ] `dev.peerfolio.near` is in your keychain (team password manager), with about 40 NEAR for the global contract's storage. It pays for the global and moves the dev factory's pointer. Arrange funding first.
- [ ] A council member can propose with their Ledger, and Peerfolio Backend has a release that handles the new version.
- [ ] `scripts/status.sh` exits 0, and `make test-integration` has run once on this machine (it downloads the sandbox).

```bash
export VERSION=<new CONTRACT_VERSION>
```

## Steps

1. **Tag the release.** CI tests and builds the tagged commit, then publishes the GitHub Release with the wasm and its hashes.
   ```bash
   git switch main && git pull
   git tag -m "Trading account $VERSION" trading-account/v$VERSION
   git push origin trading-account/v$VERSION
   ```
   Wait until the Release exists:
   ```bash
   gh release view trading-account/v$VERSION
   ```
   If CI fails because the tag doesn't match `CONTRACT_VERSION` or isn't on `main`, delete the tag (`git push origin :refs/tags/trading-account/v$VERSION`), fix it and tag again.

2. *Optional:* **rehearse on testnet.** Needs `peerfolio.peerfolio.testnet` in your keychain, which pays for the testnet global and owns the testnet factory. Then create an account and run through the [lifecycle](../trading-account.md#lifecycle).
   ```bash
   scripts/release/deploy-global.sh testnet $VERSION
   scripts/release/point-factory.sh testnet $VERSION
   ```

3. **Deploy the global contract on mainnet.** Before it sends anything, the script runs the sandbox tests against the Release's wasm (about a minute, needs the network), and stops if one fails. Run it from `main` at `$VERSION`.
   ```bash
   scripts/release/deploy-global.sh mainnet $VERSION
   ```

4. **Point the dev factory at it.**
   ```bash
   scripts/release/point-factory.sh dev $VERSION
   ```

5. ⛔ **Rehearse in dev, and release the app to prod.** Follow Peerfolio Backend's contract release runbook: new accounts, upgrades and the blocking gate on staging. Run the [smoke test](smoke-test.md). Don't go on until the app that handles `$VERSION` is on prod.

6. **Propose pointing the prod factory at it.** Without `--hd-path` it signs from the keychain; with `--print-only` it prints the call for submitting through Trezu instead.
   ```bash
   scripts/release/dao-propose.sh pointer $VERSION --as <council account> --hd-path "m/44'/397'/0'/0'/1'"
   ```

7. ⛔ **Council votes.** 3 of 4 approvals, in Trezu or the near CLI. The call runs with the third.

8. **Check.** The prod factory should point at `$VERSION`:
   ```bash
   scripts/status.sh
   ```

## If it goes wrong

- **The rehearsal (step 2 or 5) finds a problem:** prod is untouched. Delete the GitHub Release but keep the tag, so the number isn't reused (`gh release delete trading-account/v$VERSION`). Fix it, release the next version, and move dev to it. Until then, `status.sh` reports dev's pointer as unknown, because its Release is gone.
- **Move dev back** to the previous release:
  ```bash
  scripts/release/point-factory.sh dev <previous version> --rollback
  ```
- **Move prod back:** propose the previous version (`dao-propose.sh pointer <previous version> …`), and move dev back with it.
- **Accounts that already upgraded** can't be moved back, only forward. Moving the pointer back is safe only if the previous release has the same `STATE_VERSION`. A broken migration is fixed in the next release; a fix that needs a different state shape is a new state version ([rollbacks](../development.md#rollbacks)).
