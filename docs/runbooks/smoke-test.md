# Runbook: smoke test a release on mainnet

Checks a trading account release against the real `intents.near` and 1Click, which the sandbox tests stub or only quote. Run it after the dev factory points at the release ([release runbook](release-trading-account.md), step 5), on an account created from `auth.dev.peerfolio.near`.

The commands are in the [lifecycle](../trading-account.md#lifecycle). Use `auth.dev.peerfolio.near` and `network-config mainnet`. Steps marked 💸 move funds, so whoever holds the test funds runs them. A [dev session](../dev-session.md)'s menu can make the swap, `transfer_to_owner` and agent calls; decline its flash so the account keeps the release's code.

## Steps

1. **Create a trading account** from the dev factory with a fresh implicit owner ([lifecycle](../trading-account.md#lifecycle) step 1).
2. **Check its code.** `near account view-account-summary $TA network-config mainnet now` shows the release's hex hash under `Global Contract`, and `contract_version` returns the release's version.
3. **Add the agent** `bot.dev.peerfolio.near` (step 2), and check `get_agents` returns it.
4. 💸 **Fund it:** a little NEAR, and a small token balance on `intents.near`.
5. 💸 **Swap** with a live 1Click quote from staging (step 4). The transaction should succeed, and the output should arrive in the trading account's intents balance.
6. 💸 **Transfer to the owner** with `transfer_to_owner`. The owner's intents balance should go up by the amount.
7. **Remove the agent** with `remove_agent`, then try another `swap` as the agent. It should fail with `Unauthorized`.
8. **Upgrade an older account.** If the release migrates state, upgrade a dev account created before it ([upgrading](../trading-account.md#upgrading-a-trading-account)), and check that its owner and agents carried over.

Record the transaction hashes in the release's PR or the team channel.
