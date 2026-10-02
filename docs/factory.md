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

## Deploy a new factory

Use this only for a new factory account, for example when the factory's stored state changes. [`contracts/factory/deploy.sh`](../contracts/factory/deploy.sh) does it in one go:

1. It creates `auth.peerfolio.<near|testnet>` from `peerfolio.<near|testnet>` with 4 NEAR.
2. It deploys the release factory wasm from `contracts/target/near` and calls `new` with the owner, the network and the trading account code hash.
3. It checks the deployed code hash matches the local wasm.

[Get the release build](releases.md#get-the-release-build) first, then:

```bash
contracts/factory/deploy.sh peerfolio.sputnik-dao.near 6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f mainnet
```

That's how the live mainnet factory was deployed. Run it with `peerfolio.peerfolio.testnet` and `testnet` first.

- The factory ID is hardcoded to `auth.peerfolio.<suffix>`. To use another ID, edit the script, and keep the ID at most 30 characters.
- `peerfolio.<near|testnet>`'s key has to be in your keychain. If that account uses a Ledger, temporarily add a keychain full-access key, run the script, then delete the key.
- The script uses near-cli-rs's legacy-compatible commands (`near deploy`, `near state`). In that syntax, `--deposit 1` means 1 NEAR, not 1 yoctoNEAR.
- If the factory account already exists, the script only redeploys the code and ignores the owner and hash arguments. That doesn't work on mainnet, where the factory has no keys; use [Release factory code](releases.md#release-factory-code) instead.
- The previous factory, `auth-v1.peerfolio.near`, was replaced in January 2026.

Afterwards, point the app at the new factory: `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*` in ft-core (local `.env`, Render and Cloudflare).
