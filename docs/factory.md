# Factory contract

`TradingAccountFactory` ([`contracts/factory/src/lib.rs`](../contracts/factory/src/lib.rs)) creates trading accounts as sub-accounts of itself. Each new trading account runs the trading account **global contract** (NEP-591), identified by its code hash. The code is deployed once and every trading account references it, so creating one costs roughly 0.001 NEAR instead of the ~3.8 NEAR needed to store the wasm per account.

## Live deployments

Verified on chain on 2026-10-01.

| | mainnet | testnet |
|---|---|---|
| Factory | `auth.peerfolio.near` | `auth.peerfolio.testnet` |
| Factory owner | `peerfolio.sputnik-dao.near` (Peerfolio DAO, managed via Near Treasury) | `peerfolio.peerfolio.testnet` |
| Root account (creates the factory) | `peerfolio.near` | `peerfolio.testnet` |
| MPC signer | `v1.signer` | `v1.signer-prod.testnet` |
| Global code hash (bs58) | `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f` | same |
| Global code hash (hex) | `59136c3b557222ed2a30f8d02953ab50633566a49c4e107e2314f3a72c19b1f8` | same |
| Factory code (hex) | `670ecb8001989c5dd36d8ff96f45bef138f3bac417134c67ac615768c57c4b40` (CI build of `a26cd2f`, PR #166). Account is locked: no access keys. | same |
| Peerfolio agent (authorized user) | `bot.peerfolio.near` | none live (`bot.peerfolio.testnet` is configured in ft-core but doesn't exist) |
| Previous factory (legacy) | `auth-v1.peerfolio.near`, replaced January 2026 | |

Check them with `get_owner_id` / `get_signer_contract` / `get_proxy_code_base58_hash`. The agent ID is set in ft-core as `AGENT_ACCOUNT_ID` / `VITE_AGENT_ACCOUNT_ID_*`, and the factory as `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*`. Operational runbooks (changing the agent, deploying a factory, global deploys) live in ft-core: [`runbook/scenarios/automation_agent_account`](https://github.com/beneviolabs/ft-core/tree/main/runbook/scenarios/automation_agent_account).

## State

| Field | Meaning |
|---|---|
| `owner_id` | Can call `set_global_code_hash` and `add_full_access_key`. No setter. |
| `signer_contract` | MPC signer passed to every new trading account. Chosen in `new` from `network`, with no setter. |
| `global_proxy_base58_hash` | The global code hash, stored as **raw 32 bytes** (the name is misleading). |

## Methods

| Method | Who | Notes |
|---|---|---|
| `new(owner_id, network, global_proxy_base58_hash)` | init | `network` must be exactly `"mainnet"` (→ `v1.signer`) or `"testnet"` (→ `v1.signer-prod.testnet`), otherwise it panics. The hash must be bs58 for exactly 32 bytes. |
| `deposit_and_create_proxy_global(owner_id)` *payable* | `owner_id` only | **Use this to create trading accounts.** The caller must be `owner_id` and a NEAR implicit account. Requires ≥ 1,000,000 yoctoNEAR (a placeholder minimum, see the TODO in code). The whole deposit is transferred to the new account. If creation fails, `on_proxy_created` refunds the deposit to the caller. |
| `create_proxy_global(owner_id)` *payable* | `owner_id` only | The same creation step with **no minimum deposit and no refund callback**. Don't call it directly. |
| `on_proxy_created(...)` | private | Refund callback. |
| `get_base_account_name(owner_id)` | view | The sub-account prefix for an owner (rules below). Panics for anything but a NEAR implicit account. |
| `verify_implicit_base_name(owner_id, base_name)` | view | Lets frontends check that an `implicit_…` trading account belongs to an owner. |
| `set_global_code_hash(code_hash_str)` | owner | Switch the code used for **future** trading accounts. Takes bs58. |
| `add_full_access_key(public_key)` | owner | Adds a key to the factory account itself. |
| `get_owner_id`, `get_signer_contract`, `get_proxy_code_base58_hash`, `get_proxy_code_hash_hex` | view | |

What creation does, as one atomic batch on the new account: `create_account` → `transfer(deposit)` → `use_global_contract(hash)` → `new(owner_id, signer_id)` (50 Tgas). If any step fails (account already exists, bad init), none of it happens and the deposit is refunded.

### Trading account naming

The trading account ID is `implicit_<24hex>.<factory>`, for example `implicit_3f2a….auth.peerfolio.near`. Owners must be **NEAR implicit accounts** (exactly 64 lowercase hex chars). Anything else panics: named accounts (`alice.near`), ETH-implicit `0x…` accounts, uppercase hex.

`<24hex>` = the first 24 hex chars of `sha256(first 32 chars of owner_id)`. This is the original derivation, kept so existing users keep their names. Get it from `get_base_account_name` rather than computing it yourself, and once the account exists, **store its ID and use that instead of re-deriving it**.

Why these rules: named owners used to be mapped by stripping `.near`/`.testnet` and replacing dots with hyphens. That let distinct owners collide (`alice.near` vs `alice.testnet`, `sub.alice.near` vs `sub-alice.near`), and allowed anyone to create an account on a victim's behalf. Now only `owner_id` can create its own trading account. Collision risk is 96 bits of SHA-256: a random collision needs about 2^48 implicit accounts, and targeting a specific victim needs about 2^96 work (see the doc comment on `get_base_account_name`).

Other gotchas:
- **The factory account ID can be at most 30 characters.** `implicit_` + 24 hex + `.` + the factory ID must fit NEAR's 64-character limit.
- **Deposit:** the contract minimum is far below what's useful. Use about 0.12 NEAR; mainnet onboarding attaches 0.04 to 0.12 NEAR. The deposit becomes the trading account's balance, and that balance pays gas for every MPC-signed transaction.

## Build

| | Command | Output | Deploy it? |
|---|---|---|---|
| Release (reproducible) | `make release` (repo root) | `contracts/target/near/<crate>/<crate>.wasm` | **Yes**, only these |
| Dev (native) | `contracts/factory/build.sh`, `make build` | same path | No |

**Release builds** run `cargo near build reproducible-wasm` inside NEAR's official image, pinned by digest under `[package.metadata.near.reproducible_build]` in each crate's `Cargo.toml`. That gives the same hash for the same commit on any machine. The image, build command, repository and commit are embedded in the wasm (NEP-330 contract source metadata), so explorers and `cargo near` can verify a deployed contract against its source. Requirements: Docker running, and a clean tree with everything (including `Cargo.lock`) committed. To change the build environment, bump `image` and `image_digest` in **both** crates together; that changes the hashes.

**Dev builds** (`scripts/build-wasm.sh`) compile natively with the toolchain in `rust-toolchain.toml` and cargo-near 0.16.0, remapping `$CARGO_HOME` so the user's home directory doesn't leak in. They're fast and fine for tests, but the same commit produces different code on different hosts (we measured macOS arm64 vs Linux x86_64: same strings, different function count and layout), so never deploy them.

## Release workflow

### 1. Build and globally deploy the trading account

On a clean checkout of the commit you're releasing, with Docker running:

```bash
make release
```

This prints the SHA-256 (hex and bs58) of both wasms. CI runs the same build on every push and uploads the wasms as artifacts.

```bash
near contract deploy-as-global use-file contracts/target/near/trading_account/trading_account.wasm as-global-hash peerfolio.near network-config mainnet sign-with-keychain send
```

Any account can pay for the global deploy. On mainnet it costs **about 40 NEAR** (storage for the wasm), so arrange funding or reimbursement first. Record the bs58 hash it returns, along with the commit it came from. That hash is the global code hash you give the factory.

> **Hash gotchas**
> - Only `make release` hashes are verifiable. The live global hash `6ziTqYXT…` predates this: it was a native macOS build (`CARGO_NEAR_BUILD_ENVIRONMENT=host`) from the old repo layout, so it can't be reproduced. The live factory code `670ecb80…` came from the previous Docker-based CI build.
> - The 2026-10 reorg changed crate names, file paths, the build image (Rust 1.86 in NEAR's image) and the lockfile (one workspace lock), so the next release will have new hashes for both contracts regardless.
> - Rust ≥ 1.87 emits wasm that near-sandbox rejects (`PrepareError(Deserialization)`). The release image uses 1.86.

### 2a. Deploy a new factory

```bash
make release
contracts/factory/deploy.sh peerfolio.sputnik-dao.near 6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f mainnet
```

That's how the live mainnet factory was deployed. Afterwards, update `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*` in ft-core (local `.env`, Render and Cloudflare).

`deploy.sh`:
- Targets `auth.peerfolio.<near|testnet>` (factory IDs must be ≤ 30 chars, see above). If the account doesn't exist, it creates it from `peerfolio.<suffix>` with 4 NEAR. That needs `peerfolio.<suffix>`'s key in your keychain; a Ledger-secured root won't work, so temporarily add a keychain full-access key to it, deploy, then delete that key. and calls `new` with the owner, network and hash. `NETWORK` must be `mainnet` or `testnet`.
- Otherwise it **redeploys code only**. The owner and hash arguments are required but ignored on this path.
- Then compares the local wasm SHA-256 with the deployed one.

### 2b. Point an existing factory at new trading account code

On mainnet the owner is `peerfolio.sputnik-dao.near`, so this is a DAO proposal. A DAO council member creates a **Function Call** proposal in Near Treasury, and the council approves it:

```json
{
  "receiver_id": "auth.peerfolio.near",
  "actions": [
    {
      "method_name": "set_global_code_hash",
      "args": "{\"code_hash_str\":\"<new bs58 global code hash>\"}",
      "deposit": "0",
      "gas": "15000000000000"
    }
  ]
}
```

On testnet the owner can call it directly:

```bash
near contract call-function as-transaction auth.peerfolio.testnet set_global_code_hash json-args '{"code_hash_str":"<new bs58 global code hash>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as peerfolio.peerfolio.testnet network-config testnet sign-with-keychain send
```

### 2c. Update factory code

`auth.peerfolio.near` is **locked** (no access keys) and owned by the DAO, so `deploy.sh` can't redeploy it. Use a DAO proposal to add a temporary key, then deploy, then delete the key. ft-core's [2026-09-29 deployment log](https://github.com/beneviolabs/ft-core/blob/main/runbook/deployment_logs/09-29-2026-factory-deploy.md) has the exact commands:

1. Run `make release` and record the commit and the factory's hex hash.
2. Generate a keypair and store it securely.
3. A council member submits a `FunctionCall` proposal to `peerfolio.sputnik-dao.near` calling `auth.peerfolio.near.add_full_access_key({"public_key": …})` (15 Tgas), signed with their Ledger. The council votes; the quorum is 3 of 4.
4. Deploy without calling init: `near contract deploy auth.peerfolio.near use-file contracts/target/near/trading_account_factory/trading_account_factory.wasm without-init-call network-config mainnet sign-with-plaintext-private-key`
5. Check `near state auth.peerfolio.near` shows the hex hash from step 1, and that `get_owner_id`, `get_signer_contract` and `get_proxy_code_base58_hash` are unchanged.
6. Delete the temporary key: `near account delete-keys auth.peerfolio.near public-keys <key> network-config mainnet sign-with-plaintext-private-key`

Rehearse on `auth.peerfolio.testnet` first; there, `deploy.sh` with the same arguments redeploys code only, using the factory account's own key.

**Update in place** for bug fixes, gas tweaks and new view methods: anything that leaves the `TradingAccountFactory` struct unchanged.

**Deploy a new factory** (a new account) if the state struct changes. There's no migration method, so changed state can't be deserialized. The same applies to account-structure changes and major versions.

### 3. Verify

```bash
near contract call-function as-read-only auth.peerfolio.near get_proxy_code_base58_hash json-args '{}' network-config mainnet now
near contract call-function as-read-only auth.peerfolio.near get_proxy_code_hash_hex json-args '{}' network-config mainnet now
near account view-account-summary implicit_07454f3217b9229ead97798c.auth.peerfolio.near network-config mainnet now
```

The summary of a trading account created afterwards shows `Global Contract (by Hash: SHA-256 checksum hex)`, which should equal `get_proxy_code_hash_hex`.

## Existing trading accounts after a code change

Changing the global code hash **does not upgrade existing trading accounts**: they keep the code hash they were created with, and the contract has no upgrade method. To move an existing user onto new code:

1. **Planned path (Peerfolio UI):** the UI builds a `DeployContract` action, which likely takes the hex hash, and prompts each user whose trading account isn't on the expected code hash to approve it on login. This needs a DB migration (e.g. `expected_hash_version` on `user_accounts`). See ft-core's [global deploy runbook](https://github.com/beneviolabs/ft-core/blob/main/runbook/scenarios/automation_agent_account/global-deploy-trading-account-wasm.md).
2. Or have them withdraw everything (see [trading-account.md](trading-account.md#deleting-a-trading-account)) and delete the trading account, then recreate it via the factory. Confirm all assets have moved before deleting. [Withdrawal and deletion guide](https://docs.google.com/document/d/1runpneX6-h41beHh4k9FKtw3lKz6qgQbNzteR3RSCO4/).
3. More generally, any account holding a full-access key on the trading account (the owner, after `add_full_access_key`) can switch its code directly. If you do this, check that the state layout is compatible.
