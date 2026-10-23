# Autonomous Trading Account

NEAR smart contracts that give an AI agent **limited, revocable** authority to trade on a user's behalf, using only the funds the user moves into a dedicated account.

- The **factory** ([`contracts/factory`](contracts/factory)) creates one trading account per user.
- The **trading account** ([`contracts/trading-account`](contracts/trading-account)) holds the funds the agent may trade with. Agents it authorizes can swap those funds on [NEAR Intents](https://docs.near-intents.org) with 1Click quotes that pay out only to the user.

```
owner ──create──▶ factory ──creates──▶ trading account
agent ──swap(1Click quote)──▶ trading account ──mt_transfer──▶ intents.near ──▶ quote's deposit address
```

## Deployments

| | Mainnet | Testnet |
|---|---|---|
| Factory | `auth.peerfolio.near` | `auth.peerfolio.testnet` |
| Factory owner | `peerfolio.sputnik-dao.near` (DAO) | `peerfolio.peerfolio.testnet` |
| Dev factory | `auth.dev.peerfolio.near` | none |
| Dev factory owner | `dev.peerfolio.near` (team key) | none |
| Trading accounts | `implicit_<24hex>.auth.peerfolio.near`, or `implicit_<24hex>.auth.dev.peerfolio.near` from the dev factory | `implicit_<24hex>.auth.peerfolio.testnet` |
| Trading account code (global hash) | `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f` | same |
| MPC signer, used only by trading accounts on v0 | `v1.signer` | `v1.signer-prod.testnet` |
| Peerfolio agent, production | `bot.peerfolio.near` | none |
| Peerfolio agent, staging and local dev | `bot.dev.peerfolio.near` | none |

- The previous mainnet factory, `auth-v1.peerfolio.near`, was replaced in January 2026.
- The [dev factory](docs/factory.md#dev-factory) runs the same code as `auth.peerfolio.near` and points at the same globals. Peerfolio Backend's staging and local dev use it. Its pointer moves ahead of prod's, so a release is rehearsed there first.
- No production trading account authorizes `bot.dev.peerfolio.near`, so its key can't reach them.

## Terms

- **Owner**: the user's NEAR implicit account (64 hex chars). It creates and controls its trading account.
- **Agent**: an account allowed to call `swap`. At most 10 per trading account.
- **MPC key**: the key NEAR's MPC signer derives for a trading account. Trading accounts on v0 have it as a full-access key, and it signs their agents' transactions. From 1.0.0 nothing signs for the trading account, and owners should delete it.
- **Global code hash**: the hash of the trading account code deployed once as a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). The factory creates new trading accounts with it.

Some on-chain names still say "proxy" for the trading account: factory methods such as `create_proxy_global`, the factory's `global_proxy_base58_hash` argument, and some log messages. Code identifiers use the terms above.

## Docs

- [Trading account](docs/trading-account.md): security model, lifecycle with commands, and deleting an account.
- [Factory](docs/factory.md): account naming and deploying a new factory.
- [Builds and releases](docs/releases.md): release builds, what's deployed, and releasing either contract (including DAO proposals).
- [Contract reference](docs/reference.md): methods, `swap` arguments and errors.
- [Testing](docs/testing.md): local tests and CI.

## Development

Prerequisites:
- rustup. `rust-toolchain.toml` pins Rust 1.97.1, and rustup installs it automatically.
- cargo-near 0.22.0: `cargo install cargo-near --version 0.22.0 --locked`. The build scripts refuse other versions.
- [near-cli-rs](https://github.com/near/near-cli-rs), tested with 0.22. Run `near login` for each account you'll sign as.
- Docker, only to reproduce a release build locally. Releases normally use CI's build.

```bash
scripts/test.sh   # unit and integration tests
make release      # reproducible release build (CI runs it too)
make help         # all targets
```

The contracts form one Cargo workspace in `contracts/`, with a shared `Cargo.lock` and `target/`. To share code between them, add a library crate to the workspace that both depend on by path. Keep it to plain types; it must not define a `#[near]` contract. A good first candidate is the factory's `TradingAccountInitArgs`, which has to match the arguments the trading account's `new` reads. Today only a factory unit test checks the arguments the factory sends.

## Audits

The contracts deployed at `*.peerfolio.near`, including the factory deployed in January 2026, have had independent third-party security reviews. All findings were remediated. Reports: [Peerfolio Security Audits](https://www.notion.so/Security-Audits-3037541592cc80709908c49fc7649260).

Later changes aren't covered unless the reports say so.
