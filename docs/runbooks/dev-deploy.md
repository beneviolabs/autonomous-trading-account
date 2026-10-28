# Runbook: try unreleased code on one trading account

Puts a dev build of your checkout onto one trading account, lets you try it, and then puts the account back as it was. While flashed, the account runs code that no release has and nobody else can reproduce. Use it only on accounts you mean to; nothing checks whose account it is.

## Before you start

- [ ] The trading account's own full-access key is in your keychain, to flash and restore. If it isn't, add one as the owner with `add_full_access_key` ([upgrading from v0](../trading-account.md#upgrading-a-trading-account), step 1) and import it with `near account import-account`.
- [ ] The owner's key is in your keychain too, for the menu's calls (or an agent's, for `swap` and `transfer_to_owner`).
- [ ] The account has NEAR for the new code's storage: about 1 NEAR per 100 KB of wasm. An account on a global contract stores no code, so the first flash needs the full amount.

## Run a session

```bash
scripts/dev/session.sh <trading account>        # add testnet for a testnet account
```

1. It saves a snapshot to a temporary file and prints its path.
2. It flashes a dev build of your checkout (`scripts/dev/flash-dev.sh`) and asks before sending. The build leaves out the sandbox tests' quote key, and the script refuses a wasm that still trusts it. The deploy runs `migrate`, which upgrades v0 state and leaves current state as it is.
3. A menu lets you look at the account's state, swap with a live 1Click quote, call `transfer_to_owner`, add or remove an agent, or re-flash after changing the code. Every transaction is shown and asked about. Swap and `transfer_to_owner` move funds.
4. `r` restores the account to the snapshot and quits. `0` resets it to the v0 global instead (below). `q` quits without restoring and prints the command to restore later.

## What restore does

`scripts/dev/restore.sh <snapshot>` sends one transaction, signed as the trading account:
1. It deploys the restore helper, [`tools/restore-state`](../../tools/restore-state), and calls its `restore`, which writes back every storage entry from the snapshot and removes the ones written since.
2. It puts back the snapshot's code: its global contract, or the wasm the snapshot saved.

The actions share a receipt, so if any fails nothing changes, and the helper never stays on the account. Afterwards the script checks that storage and code match the snapshot. It reports keys and NEAR that differ, since those aren't contract state; send back any top-up yourself.

## Reset to v0

`scripts/dev/reset-to-v0.sh <trading account>` puts an account back on the v0 global code (`6ziT…`) with v0 state, and removes any code of its own. Use it when the account doesn't start on a global, so a snapshot restore would only bring its own code back. It sends one transaction, signed as the trading account: deploy the restore helper, call its `reset_to_v0`, then switch to the v0 global. Afterwards it checks the owner, agents and signer read back on v0.

The down migration lives only in the dev helper, never in the contract. It goes from state version 1 to v0. For a later state version, add its step down in `tools/restore-state` when you need it. To move a 1.0.0-or-later account to a release instead, the owner [upgrades](../trading-account.md#upgrading-a-trading-account) it.

## The pieces on their own

```bash
scripts/dev/snapshot.sh <trading account> > /tmp/before.json
scripts/dev/flash-dev.sh <trading account>
scripts/dev/restore.sh /tmp/before.json
scripts/dev/reset-to-v0.sh <trading account>
```

`flash-dev.sh`, `restore.sh` and `reset-to-v0.sh` take `--dry-run` to print the transaction without sending it. `snapshot.sh` only reads.

## Going back to released code without a snapshot

From 1.0.0, the owner [upgrades](../trading-account.md#upgrading-a-trading-account) the account to the hash its factory points at. That switches it back to the global contract.
