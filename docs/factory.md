# Factory

The factory creates trading accounts as sub-accounts of itself. Each one runs the trading account code that was deployed once as a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). The factory stores the code's hash and gives it to every new account. That way a new account costs about 0.001 NEAR instead of about 3.8 NEAR to store its own copy of the wasm. Accounts and owners are listed in the [README](../README.md#deployments), and methods in the [reference](reference.md#factory).

## Trading account naming

A trading account is named `implicit_<24hex>.<factory>`, for example `implicit_07454f3217b9229ead97798c.auth.peerfolio.near`. `<24hex>` is the first 24 hex characters of `sha256(first 32 characters of owner_id)`. This is the original derivation, kept so existing users keep their names. Get it from `get_base_account_name` rather than computing it yourself. Once the account exists, store its ID instead of re-deriving it.

- **Owners must be NEAR implicit accounts** (64 lowercase hex characters). Named accounts (`alice.near`), `0x…` accounts and uppercase hex are rejected.
- **Only the owner can create its own trading account.** The caller must be `owner_id`.
- **The factory account ID can be at most 30 characters**, so `implicit_` + 24 hex + `.` + the factory ID fits NEAR's 64-character limit.
- **Attach about 0.12 NEAR** when creating one. The contract minimum (1,000,000 yoctoNEAR) is a placeholder. The deposit becomes the trading account's balance, and that balance pays gas for every signed transaction. Mainnet onboarding attaches 0.04 to 0.12 NEAR.
- If creation fails (for example, the account already exists), nothing is created and the deposit is refunded.

These rules came from a security review. Named owners used to be mapped by stripping `.near`/`.testnet` and replacing dots with hyphens. That let distinct owners collide (`alice.near` and `alice.testnet`), and let anyone create an account on someone else's behalf. The collision analysis is in the doc comment on `get_base_account_name`.

## Dev factory

`auth.dev.peerfolio.near` is a second mainnet factory for Peerfolio Backend's non-production environments: staging and local dev. It runs the same factory code as `auth.peerfolio.near`, and both point at the same published globals by hash, so each release is published once. Only accounts created since it existed are on it; earlier dev accounts stay on `auth.peerfolio.near`.

Its owner is `dev.peerfolio.near`, a team account, so its pointer moves without a DAO vote. That lets every release be rehearsed in dev (new accounts, upgrades, the blocking gate) before the DAO moves prod's pointer. The keys for `dev.peerfolio.near` and `auth.dev.peerfolio.near` are in the team password manager; import them into your keychain to sign.

Both kinds of release change it, as a step before prod: [releasing trading account code](runbooks/release-trading-account.md) moves its pointer, and [releasing factory code](runbooks/release-factory.md) redeploys its code.

## Deploy a new factory

Use this only for a new factory account, for example when the factory's stored state changes. To update an existing factory's code, [release factory code](runbooks/release-factory.md) instead. [`scripts/release/new-factory.sh`](../scripts/release/new-factory.sh) does it in one go:

1. It creates the factory account from its parent account (`auth.peerfolio.near` from `peerfolio.near`) with 4 NEAR, and saves the new account's key to the keychain.
2. It deploys the factory wasm from the given commit's CI run, and calls `new` with the owner, the network and the trading account version's global hash.
3. It checks the code hash, owner and pointer, then offers to delete the key it created. A factory owned by the DAO should have no keys; decline for a dev or testnet factory, which keeps its key.

```bash
scripts/release/new-factory.sh testnet auth.peerfolio.testnet peerfolio.peerfolio.testnet <version> <commit>
scripts/release/new-factory.sh mainnet auth.peerfolio.near peerfolio.sputnik-dao.near <version> <commit>
```

`<version>` is a trading account release whose global contract is already on that network, or `v0`. On mainnet, `<commit>` must be on `main`.

- The factory ID can be at most 30 characters, and its parent's key has to be in your keychain. If the parent uses a Ledger, temporarily add a keychain full-access key to it, run the script, then delete the key.
- The previous factory, `auth-v1.peerfolio.near`, was replaced in January 2026.

Afterwards, point the app at the new factory: `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*` in Peerfolio Backend (local `.env`, Render and Cloudflare).
