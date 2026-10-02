# Autonomous Trading Account

NEAR smart contracts that give an AI agent **limited, revocable** authority to trade on a user's behalf, using only the funds the user moves into a dedicated account.

- The **factory** ([`contracts/factory`](contracts/factory)) creates one trading account per user.
- The **trading account** ([`contracts/trading-account`](contracts/trading-account)) holds the funds the agent may trade with. Agents it authorizes can get transactions signed by NEAR's MPC signer, but only to allowlisted contracts and methods.

```
owner ──create──▶ factory ──creates──▶ trading account
agent ──request_signature──▶ trading account ──sign──▶ MPC signer
agent ◀── signed transaction ──┘
agent ──broadcast──▶ NEAR ──▶ wrap.near / intents.near (sent from the trading account)
```

## Deployments

| | Mainnet | Testnet |
|---|---|---|
| Factory | `auth.peerfolio.near` | `auth.peerfolio.testnet` |
| Factory owner | `peerfolio.sputnik-dao.near` (DAO) | `peerfolio.peerfolio.testnet` |
| Trading accounts | `implicit_<24hex>.auth.peerfolio.near` | `implicit_<24hex>.auth.peerfolio.testnet` |
| Trading account code (global hash) | `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f` | same |
| MPC signer | `v1.signer` | `v1.signer-prod.testnet` |
| Peerfolio agent | `bot.peerfolio.near` | none |

## Terms

- **Owner**: the user's NEAR implicit account (64 hex chars). It creates and controls its trading account.
- **Authorized user**: an account, usually the agent, allowed to request signatures. At most 10 per trading account.
- **MPC key**: the key NEAR's MPC signer derives for a trading account. It's added as a full-access key on the trading account and signs every agent transaction.
- **Global code hash**: the hash of the trading account code deployed once as a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). The factory creates new trading accounts with it.

Older code and method names say "proxy" for the trading account.

## Docs

- [Trading account](docs/trading-account.md): security model, lifecycle and deleting an account.
- [Factory](docs/factory.md): account naming, builds, and releasing either contract.
- [Contract reference](docs/reference.md): methods, `request_signature` arguments and errors.
- [Testing](docs/testing.md): local tests and CI.

## Development

Requires rustup (the toolchain is pinned in `rust-toolchain.toml`), `cargo install cargo-near --version 0.16.0 --locked`, [near-cli-rs](https://github.com/near/near-cli-rs), and Docker for release builds.

```bash
scripts/test.sh   # unit and integration tests
make release      # reproducible wasm for deployment
make help         # all targets
```

## Audits

The contracts deployed at `*.peerfolio.near`, including the factory deployed in January 2026, have had independent third-party security reviews. All findings were remediated. Reports: [Peerfolio Security Audits](https://www.notion.so/Security-Audits-3037541592cc80709908c49fc7649260).

Later changes aren't covered unless the reports say so. That includes the factory naming and owner checks from [PR #166](https://github.com/beneviolabs/autonomous-trading-account/pull/166), which came from an internal review.
