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
| MPC signer, used only by trading accounts on v0 | `v1.signer` | `v1.signer-prod.testnet` |
| Peerfolio agent, production | `bot.peerfolio.near` | none |
| Peerfolio agent, staging and local dev | `bot.dev.peerfolio.near` | none |

- `scripts/status.sh` shows what each factory runs and which trading account release it points at.
- The previous mainnet factory, `auth-v1.peerfolio.near`, was replaced in January 2026.
- The [dev factory](docs/factory.md#dev-factory) runs the same code as `auth.peerfolio.near` and points at the same globals. Peerfolio Backend's staging and local dev use it. Its pointer moves ahead of prod's, so a release is rehearsed there first.
- No production trading account authorizes `bot.dev.peerfolio.near`, so its key can't reach them.

## Terms

- **Owner**: the user's NEAR implicit account (64 hex chars). It creates and controls its trading account.
- **Agent**: an account the owner allows to call `swap` and `transfer_to_owner`, usually Peerfolio's bot: `bot.peerfolio.near` in production, `bot.dev.peerfolio.near` in staging and local dev. At most 10 per trading account.
- **Trading account**: the per-user contract the factory creates, holding the funds agents may trade with.
- **1Click quote**: a swap quote from NEAR Intents' 1Click API, signed with 1Click's key. A live quote has a `depositAddress`; depositing `amountIn` of `originAsset` there starts the swap.
- **MPC key**: the key NEAR's MPC signer derives for a trading account. Trading accounts on v0 have it as a full-access key, and it signs their agents' transactions. From 1.0.0 nothing signs for the trading account.
- **Global code hash**: the hash of the trading account code deployed once as a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). The factory creates new trading accounts with it.

Some on-chain names still say "proxy" for the trading account: factory methods such as `create_proxy_global`, the factory's `global_proxy_base58_hash` argument, and some log messages. Code identifiers use the terms above.

## Where to go

- **Changing contract code:** [Development](docs/development.md): prerequisites, tests, builds and CI, in the order a change goes through them. To try a change on one live account, run a [dev session](docs/dev-session.md).
- **Releasing:** [Releasing](docs/releasing.md) says which release you need and links its runbook:
  - [release trading account code](docs/runbooks/release-trading-account.md),
  - [release factory code](docs/runbooks/release-factory.md),
  - [deploy a new factory](docs/runbooks/new-factory.md),
  - [smoke test a release](docs/runbooks/smoke-test.md) on mainnet.
- **Understanding the contracts:** [Trading account](docs/trading-account.md) (security model, lifecycle with commands, upgrading and deleting), [Factory](docs/factory.md) (account naming, the dev factory) and the [contract reference](docs/reference.md) (methods, `swap` arguments and errors).

```bash
make test         # unit and integration tests
make help         # all targets
```

## Audits

The contracts deployed at `*.peerfolio.near`, including the factory deployed in January 2026, have had independent third-party security reviews. All findings were remediated. Reports: [Peerfolio Security Audits](https://www.notion.so/Security-Audits-3037541592cc80709908c49fc7649260).

Later changes aren't covered unless the reports say so.
