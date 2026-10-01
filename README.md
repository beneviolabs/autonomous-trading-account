# MPC-Secured Agent Autonomy

Two NEAR smart contracts that let a user give an AI agent (or any other NEAR account) **limited, revocable** authority to transact on their behalf, using only the funds the user chooses to put at risk.

- **Factory** ([`contracts/factory`](contracts/factory)): creates a trading account for an owner, cheaply, using a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). Details: [docs/factory.md](docs/factory.md).
- **Trading account** ([`contracts/trading-account`](contracts/trading-account)): the per-user contract. Its owner authorizes users (agents), and they can ask NEAR's MPC signer to sign transactions from the trading account, but only to an allowlist of contracts and methods. Details: [docs/trading-account.md](docs/trading-account.md).

> **WARNING:** These contracts have not been audited. Use with caution.

## Glossary

These terms are used everywhere in this repo. Older docs, log strings and some on-chain method names still say "proxy"; read it as "trading account".

| Term | Meaning | In code |
|---|---|---|
| **Factory** | The contract that creates trading accounts. Deployed at `auth.peerfolio.near` / `auth.peerfolio.testnet`. | `TradingAccountFactory`, crate `trading-account-factory` |
| **Trading account** | A sub-account of the factory, `implicit_<24hex>.auth.peerfolio.near`, running the trading account global contract. Holds the funds an agent may trade with. | `TradingAccountContract`, crate `trading-account`. Legacy name: "proxy" / "auth proxy" |
| **Owner** | The user's main account, which must be a **NEAR implicit account** (64 lowercase hex chars). Controls the trading account (manages authorized users, can add full-access keys). Fixed at creation. | `owner_id` |
| **Authorized user** | An account allowed to call `request_signature`. Peerfolio's agent, `bot.peerfolio.near`, is the typical authorized user. Max 10 per trading account. | `authorized_users` |
| **MPC signer** | NEAR's chain-signatures contract: `v1.signer` (mainnet), `v1.signer-prod.testnet` (testnet). | `signer_id` (trading account), `signer_contract` (factory) |
| **MPC key** | The secp256k1 key the MPC signer derives for (trading account, derivation path). Registered as a full-access key on the trading account. Every signed transaction uses it. | `mpc_signer_pk`, `derivation_path` |
| **Global code hash** | bs58 SHA-256 of the trading account wasm deployed as a global contract. The factory creates new trading accounts with this code. | `global_proxy_base58_hash` (raw 32 bytes despite the name), `get_proxy_code_base58_hash` |
| **Allowlist** | The contracts and methods a signed transaction may target. Hardcoded. | `ALLOWED_CONTRACTS`, `ALLOWED_METHODS` in `actions.rs` |

## Live deployments (mainnet)

| | |
|---|---|
| Factory | `auth.peerfolio.near` (owner `peerfolio.sputnik-dao.near`) |
| Trading accounts | `implicit_<24hex>.auth.peerfolio.near`, e.g. `implicit_07454f3217b9229ead97798c.auth.peerfolio.near` |
| Global code hash | `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f` |
| MPC signer | `v1.signer` |
| Agent | `bot.peerfolio.near` |

Testnet mirrors this at `auth.peerfolio.testnet`. See [docs/factory.md](docs/factory.md#live-deployments) for the full table, including the legacy `auth-v1.peerfolio.near` factory.

## How it fits together

```
owner (<64-hex>) ──deposit_and_create_proxy_global──▶ factory (auth.peerfolio.near)
                                                          │ creates + inits
                                                          ▼
agent (authorized user) ──request_signature──▶ trading account (implicit_<24hex>.auth.peerfolio.near)
                                                          │ checks allowlist, builds tx, calls sign
                                                          ▼
                                               MPC signer (v1.signer) ──signature──▶ trading account
                                                                                     │ returns base64 signed tx
agent ◀──────────────────────────────────────────────────────────────────────────────┘
agent ──broadcasts signed tx──▶ NEAR ──▶ wrap.near / intents.near (sent from the trading account)
```

The trading account **never broadcasts anything itself**. It returns a signed transaction, and the caller has to broadcast it. That transaction is sent *from* the trading account using its MPC key, so gas is paid from the trading account's balance.

## Repository layout

```
contracts/                 Cargo workspace: one Cargo.lock, one target/ dir
  Cargo.toml               workspace members + shared release/test profiles
  .cargo/audit.toml        cargo-audit ignores (with reasons)
  factory/                 factory contract (crate trading-account-factory)
    src/lib.rs             TradingAccountFactory
    src/unit_tests.rs
    build.sh               dev build
    deploy.sh              create a new auth.peerfolio.<suffix> factory
  trading-account/         trading account contract (crate trading-account)
    src/lib.rs             TradingAccountContract
    src/actions.rs         allowlist
    src/models.rs          MPC request/response + NEAR transaction types
    src/serializer.rs, src/utils.rs
    src/unit_tests.rs, src/integration_tests.rs (behind the `integration-tests` feature)
    build.sh               dev build
  target/near/<crate>/<crate>.wasm   build output, e.g. trading_account/trading_account.wasm
scripts/
  build-wasm.sh            native dev build of one crate
  test.sh                  unit + integration tests
docs/                      factory.md, trading-account.md, testing.md
Makefile                   release, build, test, lint, audit (`make help`)
.github/workflows/contracts.yml   CI
```

To share code between the contracts, add a library crate to the workspace and depend on it by path from both. Keep it to plain types; it must not define a `#[near]` contract. A good first candidate is the factory's `ProxyInitArgs`, which has to match the trading account's `new(owner_id, signer_id)` and isn't checked by the compiler today.

## Builds and hashes

- **Release builds are reproducible.** `make release` runs `cargo near build reproducible-wasm` for both contracts inside NEAR's official build image, pinned by digest under `[package.metadata.near.reproducible_build]` in each `Cargo.toml`. Anyone can rerun it on the same commit and get the same hash. The image, command, repo and commit are embedded in the wasm (NEP-330), so explorers can verify it. It needs Docker and a clean, committed tree. **Only deploy release builds.**
- **Dev builds are not.** `make build`, the `build.sh` scripts and `scripts/test.sh` compile natively, which is fast, but the same commit produces different code on different hosts (macOS arm64 vs Linux x86_64). Use these for tests only.

## Prerequisites

1. [near-cli-rs](https://github.com/near/near-cli-rs), tested with 0.22.x. `deploy.sh` uses its legacy `near deploy` / `near state` compatibility commands. In legacy syntax `--deposit 1` means **1 NEAR**, not 1 yocto.
2. Rust via rustup. `rust-toolchain.toml` pins the version and rustup installs it automatically. Don't build with Rust ≥ 1.87: near-sandbox rejects its wasm.
3. `cargo install cargo-near --version 0.16.0 --locked`. `scripts/build-wasm.sh` refuses other versions.
4. Docker, only for `make release`.
5. `near login` with each account you will sign as, saving keys to the keychain.
6. Testnet funds: [near-faucet.io](https://near-faucet.io/).

## End-to-end walkthrough (testnet)

This walks through the full lifecycle from an empty setup. Each step links to the detailed doc. The factory is `auth.peerfolio.testnet`. The owner is `$OWNER`, a funded NEAR implicit account; named accounts like `alice.testnet` are rejected. To make one, run `near account create-account fund-later use-auto-generation save-to-folder <dir>`, send NEAR to the printed 64-hex ID, and sign as it with `sign-with-access-key-file <dir>/<id>.json` in place of `sign-with-keychain` below. There's no live testnet agent, so `$AGENT` is any testnet account you control.

**Operator: ship the contracts** (mainnet, only when the code changes; see [docs/factory.md](docs/factory.md#release-workflow))

1. On a clean checkout of the release commit, run `make release`. Record the commit and the hashes it prints; these go in DAO proposals.
2. Deploy the trading account wasm as a global contract (by hash). It costs about 40 NEAR on mainnet. Note the bs58 hash it returns.
   ```bash
   near contract deploy-as-global use-file contracts/target/near/trading_account/trading_account.wasm as-global-hash peerfolio.near network-config mainnet sign-with-keychain send
   ```
3. Point `auth.peerfolio.near` at it with a Near Treasury DAO proposal calling `set_global_code_hash`. Only a brand-new factory uses `contracts/factory/deploy.sh peerfolio.sputnik-dao.near <hash> mainnet`.
4. Verify that `auth.peerfolio.near`'s `get_proxy_code_base58_hash` equals the hash from step 2.
5. Existing trading accounts stay on the old code until their owners approve a redeploy (see [docs/factory.md](docs/factory.md#existing-trading-accounts-after-a-code-change)).

The rest of the walkthrough uses testnet, which is safe to experiment on.

**Owner: onboard** (see [docs/trading-account.md](docs/trading-account.md#onboarding))

6. Look up the trading account name, then create it. The **owner must sign** the create call.
   ```bash
   near contract call-function as-read-only auth.peerfolio.testnet get_base_account_name json-args "{\"owner_id\":\"$OWNER\"}" network-config testnet now
   ```
   ```bash
   near contract call-function as-transaction auth.peerfolio.testnet deposit_and_create_proxy_global json-args "{\"owner_id\":\"$OWNER\"}" prepaid-gas '300.0 Tgas' attached-deposit '0.12 NEAR' sign-as $OWNER network-config testnet sign-with-keychain send
   ```
   The result is `TA=implicit_<24hex>.auth.peerfolio.testnet`. Store this ID; don't re-derive it later.
7. Derive the MPC key. By convention the derivation path is the trading account ID.
   ```bash
   near contract call-function as-read-only v1.signer-prod.testnet derived_public_key json-args "{\"path\":\"$TA\",\"predecessor\":\"$TA\",\"domain_id\":0}" network-config testnet now
   ```
8. As the owner, register that key and authorize the agent:
   ```bash
   near contract call-function as-transaction $TA add_full_access_key json-args '{"public_key":"secp256k1:<MPC key>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-keychain send
   ```
   ```bash
   near contract call-function as-transaction $TA add_authorized_user json-args "{\"account_id\":\"$AGENT\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-keychain send
   ```
9. Fund the trading account. It pays gas for every signed transaction, plus any deposits those transactions attach.
   ```bash
   near tokens $OWNER send-near $TA '0.2 NEAR' network-config testnet sign-with-keychain send
   ```

**Agent: act** (see [docs/trading-account.md](docs/trading-account.md#request_signature-arguments))

10. Get the MPC key's current nonce on the trading account (`near account list-keys $TA network-config testnet now`) and a recent block hash (e.g. from [testnet.nearblocks.io](https://testnet.nearblocks.io)).
11. As the agent, request a signed transaction that wraps 0.05 NEAR. `actions_json` is a JSON *string*, and `nonce` is the key's nonce + 1.
    ```bash
    near contract call-function as-transaction $TA request_signature json-args "{\"contract_id\":\"wrap.testnet\",\"actions_json\":\"[{\\\"type\\\":\\\"FunctionCall\\\",\\\"method_name\\\":\\\"near_deposit\\\",\\\"args\\\":{},\\\"gas\\\":\\\"30000000000000\\\",\\\"deposit\\\":\\\"50000000000000000000000\\\"}]\",\"nonce\":\"<nonce + 1>\",\"block_hash\":\"<block hash>\",\"mpc_signer_pk\":\"secp256k1:<MPC key>\",\"derivation_path\":\"$TA\"}" prepaid-gas '300.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $AGENT network-config testnet sign-with-keychain send
    ```
    The return value (also logged as `Signed transaction (base64): …`) is the signed transaction.
12. Broadcast it:
    ```bash
    near transaction send-signed-transaction '<base64>' network-config testnet
    ```

**Owner: offboard** (see [docs/trading-account.md](docs/trading-account.md#deleting-a-trading-account))

13. Withdraw tokens held in `wrap.*` / `intents.near` first, then add your own full-access key and delete the account with your main account as beneficiary.

### Example transactions

These predate the implicit-owner naming.

- Testnet: [converting 1 NEAR to wNEAR](https://testnet.nearblocks.io/txns/Hi2pfe89tBdMN2oY2dFXLuHcSBVFotx6pHViDQuKUZDi), signed via a [request_signature call](https://testnet.nearblocks.io/txns/831u2KqbdtzvJti5HUhGnp4tZD7Q8onUzD11rwBjrAAm) by authorized user `benevio-labs.testnet` (older API version).
- Mainnet: [adding a public key on `intents.near`](https://nearblocks.io/txns/GRw6oEWjAQ2QT9oDtsgBSRWr3s4oCW4A8zCpHCRXD62s), signed via [this request](https://nearblocks.io/txns/9PJXbvcb4RMxjwK8VW4N54RnvrjENUCr6N1nv9f3DZJQ).
- Onboarding: [create trading account](https://testnet.nearblocks.io/txns/CF1ainGjroxtppNTWWkFgQsiC5kC4iJ3X7v8FgLMrWDE?tab=execution), [MPC key registration](https://testnet.nearblocks.io/txns/5Jyn459DhAaRxEqvTo3x724cgxCpjTH1Jaoc7uyVNQt9).

## Top gotchas

The detailed docs explain each of these.

- **The allowlist restricts the receiver contract and method, not the arguments.** An authorized user can call `ft_withdraw` / `mt_transfer` on `intents.near` to *any* recipient. Only authorize accounts you trust with the trading account's funds.
- **The owner can't call `request_signature` unless it adds itself as an authorized user.** `is_authorized` includes the owner, but `request_signature` checks only the authorized-user list.
- **Only NEAR implicit accounts can own trading accounts, and they must create their own.** Named accounts, ETH-implicit accounts and creating on someone else's behalf all panic.
- **Changing the factory's global code hash only affects trading accounts created afterwards.** Existing trading accounts keep their code. There's no in-contract upgrade path.
- **Only deploy `make release` builds.** Native dev builds produce different code on different machines, so their hashes can't be verified.
- **Mainnet factory owner is `peerfolio.sputnik-dao.near`**, so owner-only factory calls on mainnet go through a DAO proposal.

## Audits

The contracts deployed at `*.peerfolio.near`, including the factory at `auth.peerfolio.near` deployed in January 2026, have had independent third-party security reviews. All findings were remediated, verified and tested. Reports: [Peerfolio Security Audits](https://www.notion.so/Security-Audits-3037541592cc80709908c49fc7649260).

Later changes aren't covered by those reviews unless the reports say so. That includes PR #166's factory naming and owner checks, which came from an internal security review on 2026-08-28. A review reduces risk but doesn't eliminate it.
