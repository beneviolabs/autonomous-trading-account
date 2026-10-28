# Runbook: release factory code

Redeploys new factory code onto the existing factories, keeping their stored state: owner, MPC signer and trading account pointer. Only for changes that read the same stored fields, such as bug fixes, gas tweaks and new view methods. If the `TradingAccountFactory` struct changed, [deploy a new factory](new-factory.md) instead. Background: [Releasing](../releasing.md).

Every script prints the transaction and asks before sending it. Add `--dry-run` to only print.

## Before you start

- [ ] The change is merged to `main`, and the `contracts` run for the merge commit passed. If there's no push run for it, start one: `gh workflow run contracts.yml --ref main`. CI artifacts expire after 90 days.
- [ ] `auth.dev.peerfolio.near`'s key is in your keychain (team password manager).
- [ ] A council member will deploy prod, signing with their Ledger.
- [ ] `scripts/status.sh` exits 0.

```bash
export COMMIT=<merge commit on main>
```

## Steps

1. *Optional:* **rehearse on testnet**, signing as `auth.peerfolio.testnet` from the keychain. Any branch's commit works here, once it has a successful push or manual CI run (`gh workflow run contracts.yml --ref <branch>`). Then create a trading account ([lifecycle](../trading-account.md#lifecycle) step 1).
   ```bash
   scripts/release/deploy-factory.sh testnet $COMMIT
   ```

2. **Deploy to the dev factory.** The script checks the code hash, and that the owner, signer and pointer haven't changed.
   ```bash
   scripts/release/deploy-factory.sh dev $COMMIT
   ```
   ⛔ Check that a new dev login onboards.

3. **Propose a temporary key for the prod factory.** The prod factory has no keys, so the deploying council member's own Ledger key is added for the deploy. Get it with `near account get-public-key from-ledger`.
   ```bash
   scripts/release/dao-propose.sh add-key <your Ledger public key> --as <council account> --hd-path "m/44'/397'/0'/0'/1'"
   ```

4. ⛔ **Council votes.** 3 of 4 approvals, in Trezu or the near CLI.

5. **Deploy to prod.** The script signs with the key from step 3, runs the same checks as step 2, and then deletes the key.
   ```bash
   scripts/release/deploy-factory.sh prod $COMMIT --hd-path "m/44'/397'/0'/0'/1'"
   ```

6. **Check.** `status.sh` exits 0 only if the prod factory has no keys left:
   ```bash
   scripts/status.sh
   ```

## If it goes wrong

- **The deploy in step 5 fails:** the key stays on the factory, so you can fix the cause and run step 5 again without another vote. Running it again after a deploy that worked only deletes the key.
- **The checks after a deploy fail:** redeploy the previous factory code the same way, with the commit it was built from.
- **A key is left on the prod factory:** `status.sh` flags it. Delete it with the key itself:
  ```bash
  near account delete-keys auth.peerfolio.near public-keys <key> network-config mainnet sign-with-ledger --seed-phrase-hd-path "m/44'/397'/0'/0'/1'" send
  ```
