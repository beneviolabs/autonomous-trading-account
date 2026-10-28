# Releasing

How new code reaches testnet and mainnet, and which runbook to follow. How release builds are made is in [Development](development.md#builds). Accounts and owners are in the [README](../README.md#deployments).

## Which release do I need?

| What changed | Runbook | Affects |
|---|---|---|
| Trading account code (`contracts/trading-account`) | [Release trading account code](runbooks/release-trading-account.md) | Trading accounts created afterwards, and existing ones once their owner [upgrades](trading-account.md#upgrading-a-trading-account). The factory code isn't touched. |
| Factory code (`contracts/factory`), with the same stored fields | [Release factory code](runbooks/release-factory.md) | The factory only. It keeps its owner, MPC signer and trading account code hash. |
| The factory's stored state, or a new factory account is needed | [Deploy a new factory](runbooks/new-factory.md) | A new factory. Accounts created by the old one stay where they are. |

A release never changes an existing trading account by itself. Each keeps its code until its owner [upgrades it](trading-account.md#upgrading-a-trading-account) to the hash the factory points at.

- **Trading account code** goes out as a new global contract, and the factories are then pointed at its hash: the [dev factory](factory.md#dev-factory) first, prod through the DAO once the dev rehearsal passes and the app that handles the new version is on prod.
- **Factory code** is redeployed onto the existing factories, dev first, keeping their stored state.

Who's involved on mainnet: the factory owner is the DAO `peerfolio.sputnik-dao.near`, so every change to `auth.peerfolio.near` is a [DAO proposal](#dao-proposals). On testnet, the factory owner `peerfolio.peerfolio.testnet` and the factory account `auth.peerfolio.testnet` sign directly from the keychain. The mainnet dev factory `auth.dev.peerfolio.near` and its owner `dev.peerfolio.near` sign from the keychain too.

To see what each factory runs and points at now, run `scripts/status.sh` (or `scripts/status.sh testnet`).

## Versions

The trading account reports its version from `contract_version()`. The numbers are two constants at the top of `contracts/trading-account/src/lib.rs`:
- `CONTRACT_VERSION`: bumped once per release, as a patch, minor or major ([which version to bump](development.md#which-version-to-bump)).
- `STATE_VERSION`: bumped only in the PR that changes the shape of the stored state ([what needs a new state version](development.md#what-needs-a-new-state-version)). A unit test fails if the state changes without it.

**A release is a git tag**, `trading-account/v<CONTRACT_VERSION>`. Pushing the tag is what releases. CI then:
1. runs the tests and the reproducible build of the tagged commit,
2. checks the tag is on `main`, matches `CONTRACT_VERSION` in that commit and is the newest release tag,
3. publishes a GitHub Release holding `trading_account.wasm` and its hashes.

GitHub Releases are kept forever, unlike CI artifacts (90 days), so each release's wasm and hashes stay one click from its version. A version can't be released twice: a second Release for the same tag fails. Skipped version numbers are fine.

### v0

The trading account code that accounts created before 1.0.0 run, global hash `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f`, has no GitHub Release. It was a native macOS build from before the repo reorg and can't be reproduced, so the release-gate tests use a copy fetched from mainnet, `contracts/trading-account/res/v0.wasm`, and the scripts know its hash as `v0`.

### 1.0.0

The first versioned release, and the first `upgrade` and `migrate`. Agents swap with `swap(quote_payload, quote_signature)`, which deposits into a 1Click quote with a direct `mt_transfer` call. The owner and agents can move up to 10 tokens from the trading account's intents balance to the owner's with `transfer_to_owner(tokens, memo)`. `request_signature`, the MPC signing path, `get_signer_id` and `set_signer_id` are gone, and migrating from v0 drops `signer_id` from the state.

## DAO proposals

Each mainnet change to the prod factory is one function call on `auth.peerfolio.near`, proposed to the DAO with `scripts/release/dao-propose.sh`. It needs 3 of 4 council approvals, and council members vote however they like, in Trezu or the near CLI.

`auth.peerfolio.near` has no access keys, so to deploy factory code the DAO adds the deploying council member's key, they deploy with it, and the script deletes it.

> **TODO: deploy factory code without a temporary key.** For the length of a release, one council member alone holds a full-access key to the factory, outside the DAO's 3-of-4 approval. Instead, the factory could have an owner-only method that deploys the wasm passed to it onto its own account. Sputnik DAO already supports this: `store_blob` uploads the wasm to the DAO, and an `UpgradeRemote` proposal calls that method with it once approved. The factory would never need an access key. That's a factory code change, so it needs review, and the first release that adds the method still has to go out this way.
