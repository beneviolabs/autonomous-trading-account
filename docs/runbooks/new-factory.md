# Runbook: deploy a new factory

Creates and initializes a new factory account. Use it only when the factory's stored state changes, or another factory is needed. To update an existing factory's code, [release factory code](release-factory.md) instead. Background: [Factory](../factory.md), [Releasing](../releasing.md).

The script prints each transaction and asks before sending it. Add `--dry-run` to only print.

## Before you start

- [ ] The factory change is merged to `main` (on mainnet), and the `contracts` run for that commit passed.
- [ ] The trading account version the factory will point at is a global contract on that network, or is `v0`.
- [ ] The new factory ID is at most 30 characters, so trading account IDs fit NEAR's 64.
- [ ] The parent account's key is in your keychain: `peerfolio.near` for `auth.peerfolio.near`, `dev.peerfolio.near` for `auth.dev.peerfolio.near`, `peerfolio.testnet` for `auth.peerfolio.testnet`. If the parent uses a Ledger, temporarily add a keychain full-access key to it, and delete it afterwards.

## Steps

1. *Optional:* **rehearse on testnet.**
   ```bash
   scripts/release/new-factory.sh testnet auth.peerfolio.testnet peerfolio.peerfolio.testnet <version> <commit>
   ```

2. **Create it.** The script:
   1. creates the factory account from its parent with 4 NEAR, and saves the new account's key to the keychain,
   2. deploys the factory wasm from the commit's CI run, and calls `new` with the owner, the network and the version's global hash,
   3. checks the code hash, owner and pointer,
   4. offers to delete the key it created. A factory owned by the DAO should have no keys; decline for a dev or testnet factory, which keeps its key.
   ```bash
   scripts/release/new-factory.sh mainnet auth.peerfolio.near peerfolio.sputnik-dao.near <version> <commit>      # prod
   scripts/release/new-factory.sh mainnet auth.dev.peerfolio.near dev.peerfolio.near <version> <commit>          # dev
   ```
   If it stops after creating the account, run it again: it carries on from the deploy.

3. **Point the app at it:** `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*` in Peerfolio Backend (local `.env`, Render and Cloudflare).

4. **Check.** Add the new factory to `factory_target` in `scripts/lib.sh` if it replaces one there, then run `scripts/status.sh`.

The previous factory, `auth-v1.peerfolio.near`, was replaced in January 2026.
