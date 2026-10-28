# Runbook: deploy unreleased code to one trading account

Puts a dev build of your checkout straight onto one trading account, to try code before it's released. The account then runs code that no release has, and that nobody else can reproduce. Use it only on accounts you mean to; it doesn't check whose account it is.

## Before you start

- [ ] The trading account's own full-access key is in your keychain. If it isn't, add one as the owner with `add_full_access_key` ([upgrading from v0](../trading-account.md#upgrading-a-trading-account), step 1) and import it with `near account import-account`.
- [ ] The account has enough NEAR for the new code's storage: about 1 NEAR per 100 KB of wasm. Accounts on a global contract store no code, so the first flash needs the full amount.

## Steps

1. **Flash it.** Builds the trading account from your checkout without the sandbox tests' quote key, and refuses if the wasm still trusts it. Deploys with `migrate`, which upgrades v0 state and leaves current state as it is. Then checks the code hash, version, owner and agents.
   ```bash
   scripts/flash-dev.sh <trading account>
   ```
   Add `testnet` after the account for a testnet account, and `--dry-run` to only print the transaction.

2. **Check** what it says it did, for example on [nearblocks](https://nearblocks.io).

## Going back to released code

From 1.0.0, the owner [upgrades](../trading-account.md#upgrading-a-trading-account) the account to the hash its factory points at. That switches it back to the global contract.
