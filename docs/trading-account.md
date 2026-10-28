# Trading account

The per-user contract, created by the [factory](factory.md) at `implicit_<24hex>.auth.peerfolio.near`, or `implicit_<24hex>.auth.dev.peerfolio.near` from the [dev factory](factory.md#dev-factory). The owner moves in only the funds an agent may trade with. Agents can then swap them on `intents.near` with 1Click quotes that pay out only to the user. Method details are in the [reference](reference.md#trading-account).

## Glossary

- **Owner**: the user's NEAR account that controls the trading account.
- **Agent**: an account the owner allows to call `swap`, usually Peerfolio's bot: `bot.peerfolio.near` in production, `bot.dev.peerfolio.near` in staging and local dev. A trading account can have at most 10.
- **1Click quote**: a swap quote from NEAR Intents' 1Click API, signed with 1Click's key. A live quote has a `depositAddress`; depositing `amountIn` of `originAsset` there starts the swap.
- **Trading account**: this contract.

## Security model

- **Who can swap.** The owner and its agents, each with exactly 1 yoctoNEAR attached, so the call must be signed with a full-access key. Nobody else.
- **The trading account signs nothing.** `swap` calls `mt_transfer` on `intents.near` itself, so `intents.near` sees the trading account as the sender. There are no signed transactions to hoard, and no MPC key is needed.
- **The quote decides the transfer.** The caller passes only a 1Click quote and its signature. The contract sends the quote's `amountIn` of its `originAsset` to its `depositAddress`, and checks that:
  - 1Click signed it. The key is compiled in ([`quote.rs`](../contracts/trading-account/src/quote.rs)), so rotating it takes a release and an upgrade by each owner. Once the signature verifies, the quote's values are trusted as 1Click's; the contract doesn't sanity-check amounts.
  - It's live, not dry. Its `recipient` and `refundTo` are each the trading account or the owner, and its deposit, recipient and refund types are all `INTENTS`. It names no virtual-chain recipient or custom recipient message. Its `slippageTolerance` is at most 200 bps.
  - Its `deadline` is after the block time. That's the quote's deposit deadline, which 1Click sets (3 days in the live test). There's no limit on the quote's age.
- **A valid quote can be reused** until its deadline. Each use is another deposit to the same address, which 1Click swaps or refunds to `refundTo`. This is accepted by design: the swapped output and refunds still only go back to the user, though each use pays the quote's fees again (see below).
- **Cash-out** is a swap with `recipient` set to the owner (liquidation), followed by the owner's own `ft_withdraw` from intents. The trading account has no withdraw method.
- **The contract doesn't protect funds from a compromised agent.** It makes sure a swap's output and refunds go to the user's accounts, but it doesn't limit how much value a swap loses on the way. Anyone holding an agent key, such as a stolen Peerfolio bot key, can drain the account a little at a time ([#189](https://github.com/beneviolabs/autonomous-trading-account/issues/189)):
  - **Fees.** Whoever requests a quote can add `appFees` paid to any address, up to 4.75% of the swap (1Click caps all fees at 5%, its own share included). 1Click doesn't sign `appFees`, so the contract can't see the fee or who gets it. The fee comes out of the deposit, so the user receives less. The same gap lets a thief quietly redirect Peerfolio's own fee.
  - **Prices.** Each swap can lose up to 2% to slippage, and more at bad prices on illiquid tokens. The contract has no reference price.
  - **Repetition.** Nothing limits how often an agent swaps, so it can swap back and forth, or reuse a quote, paying fees and slippage every time.
  - **1Click's key.** A compromised 1Click key would get past the quote check entirely.

  Only add an agent you'd trust with the trading account's funds.
- **Keys registered on `intents.near` bypass the contract.** Any public key registered for the trading account on `intents.near` can sign intents that move its balance anywhere. `delete_key` doesn't unregister a key; that takes `intents.near`'s `remove_public_key`, called by the account.
- **Only upgraded accounts get these checks.** Trading accounts on v0 have no `swap`, and sign any call on v0's wider allowlist, including `ft_withdraw` and `mt_transfer` to any receiver.
- **To cut off an agent**, remove it with `remove_agent`. It takes effect at once.
- The checks are compiled in. Changing them means [releasing new trading account code](releases.md#release-trading-account-code), and existing trading accounts keep their old code until their owner [upgrades them](#upgrading-a-trading-account).

## Lifecycle

These commands use testnet and near-cli-rs (tested with 0.22). Variables:
- `$OWNER`: a funded NEAR implicit account. To make one, run `near account create-account fund-later use-auto-generation save-to-folder <dir>` and send NEAR to the 64-hex ID it prints ([faucet](https://near-faucet.io/)).
- `$OWNER_KEY`: the owner's key file, `<dir>/<id>.json`. Commands signed by the owner use `sign-with-access-key-file $OWNER_KEY`.
- `$AGENT`: the agent. Peerfolio's mainnet agents are `bot.peerfolio.near` (production) and `bot.dev.peerfolio.near` (staging and local dev). There's no live testnet agent, so use any testnet account you control.
- `$TA`: the trading account ID.

1. **Create** the trading account. The owner must sign. Look up its name first:
   ```bash
   near contract call-function as-read-only auth.peerfolio.testnet get_base_account_name json-args "{\"owner_id\":\"$OWNER\"}" network-config testnet now
   ```
   ```bash
   near contract call-function as-transaction auth.peerfolio.testnet deposit_and_create_proxy_global json-args "{\"owner_id\":\"$OWNER\"}" prepaid-gas '300.0 Tgas' attached-deposit '0.12 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
   `$TA` is `<name>.auth.peerfolio.testnet`. Store it; don't re-derive it later.
2. **Authorize the agent**, as the owner:
   ```bash
   near contract call-function as-transaction $TA add_agent json-args "{\"account_id\":\"$AGENT\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
3. **Fund it.** Move the tokens the agent may trade into the trading account's `intents.near` balance, and keep a little NEAR in the account for its storage. The caller of `swap` pays its gas and the 1 yocto.
   ```bash
   near tokens $OWNER send-near $TA '0.2 NEAR' network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
4. **Trade.** The agent gets a live 1Click quote whose `recipient` and `refundTo` are the trading account (or the owner, to liquidate), and passes it to `swap`:

   ```mermaid
   sequenceDiagram
       participant Agent as Agent
       participant OneClick as 1Click
       participant TA as Trading account
       participant Intents as intents.near

       Agent->>OneClick: quote (dry: false)
       OneClick-->>Agent: signed quote with depositAddress
       Agent->>TA: swap(quote_payload, quote_signature)
       TA->>TA: check caller, signature and terms
       TA->>Intents: mt_transfer(depositAddress, originAsset, amountIn)
       Intents-->>TA: result, checked by swap_callback
       OneClick->>Intents: swap, delivered to recipient
   ```

   `quote_payload` is the exact JSON 1Click signed, a JSON *string*:
   ```bash
   near contract call-function as-transaction $TA swap json-args "{\"quote_payload\":\"<payload JSON, escaped>\",\"quote_signature\":\"ed25519:<signature>\"}" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $AGENT network-config mainnet sign-with-keychain send
   ```
   1Click and `intents.near` only exist on mainnet, so this step can't be run on testnet. If something fails, see the [common failures](reference.md#swap).

Example transactions:
- Mainnet: [a `swap` of 0.02 wNEAR](https://nearblocks.io/txns/GcStxtskzJQfMsphnA1AUogZWncmT7B9PNPoPR72M1g5), from a prototype of 1.0.0.

From before the implicit-owner naming and 1.0.0:
- Testnet: [wrapping 1 NEAR](https://testnet.nearblocks.io/txns/Hi2pfe89tBdMN2oY2dFXLuHcSBVFotx6pHViDQuKUZDi), signed through [this `request_signature` call](https://testnet.nearblocks.io/txns/831u2KqbdtzvJti5HUhGnp4tZD7Q8onUzD11rwBjrAAm) (older API).
- Mainnet: [adding a public key on `intents.near`](https://nearblocks.io/txns/GRw6oEWjAQ2QT9oDtsgBSRWr3s4oCW4A8zCpHCRXD62s), signed through [this request](https://nearblocks.io/txns/9PJXbvcb4RMxjwK8VW4N54RnvrjENUCr6N1nv9f3DZJQ).
- Onboarding: [creating a trading account](https://testnet.nearblocks.io/txns/CF1ainGjroxtppNTWWkFgQsiC5kC4iJ3X7v8FgLMrWDE?tab=execution), [registering the MPC key](https://testnet.nearblocks.io/txns/5Jyn459DhAaRxEqvTo3x724cgxCpjTH1Jaoc7uyVNQt9).

## Replacing an agent

The owner adds the new agent with `add_agent` and removes the old one with `remove_agent`, in either order; both can go in one transaction to the trading account.

The trading account code deployed now (see the [README](../README.md#deployments)) predates 1.0.0 until its owner [upgrades it](#upgrading-a-trading-account): it uses `add_authorized_user` and `remove_authorized_user`, signs through the MPC key, and can't delete that key. Removing the old agent still stops it requesting new signatures, but what it already had signed stays valid until it expires, about 24 hours later.

## Upgrading a trading account

A [release](releases.md#release-trading-account-code) changes only the code of trading accounts created afterwards. An existing account keeps its code until its owner upgrades it to the code the factory points at. The upgrade switches the code and runs `migrate` in one receipt, so if `migrate` fails the account keeps its old code and state. The agents and the owner carry over. Upgrading from v0 drops the MPC signer from the state; the MPC key stays on the account.

Get the hash the account's own factory (its parent) points at, and check it matches the hash in the version's [GitHub Release](releases.md#versions):
```bash
near contract call-function as-read-only auth.peerfolio.testnet get_proxy_code_base58_hash json-args '{}' network-config testnet now
```

**From 1.0.0 onwards**, the owner calls `upgrade` with that hash. It fails if the factory points anywhere else by the time it runs:
```bash
near contract call-function as-transaction $TA upgrade json-args '{"expected_hash":"<bs58 hash>"}' prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
```

These commands are for testnet. On mainnet, use `network-config mainnet` and query the account's parent, the factory its ID ends in: `auth.peerfolio.near`, or `auth.dev.peerfolio.near` for dev accounts created on the dev factory. Older dev accounts are on `auth.peerfolio.near`. `upgrade` checks the hash against its parent, so querying the other factory can make it fail.

**From the original code (v0)**, which has no `upgrade` and no `contract_version`, the owner sends the same switch from the trading account itself, with a temporary key:
1. As the owner, add your own public key to the trading account:
   ```bash
   near contract call-function as-transaction $TA add_full_access_key json-args '{"public_key":"<your public key>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
2. In one transaction signed with that key, switch to the hash, run `migrate` and delete the key. It prompts for the key pair you added in step 1. Nothing checks the hash for you here, so use the factory's. If `migrate` fails, the whole transaction reverts and the key is still there to retry with:
   ```bash
   near transaction construct-transaction $TA $TA add-action use-global-contract use-global-hash <bs58 hash> with-init-call migrate json-args '{}' prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' add-action delete-key '<your public key>' skip network-config testnet sign-with-plaintext-private-key send
   ```

Either way, check the result:
- `near account view-account-summary $TA network-config testnet now` shows the new hash, in hex, under `Global Contract`, and only the keys the account had before.
- `contract_version` returns the release's version, and `get_agents` returns the same agents as before.
- The transaction logs a `migrated` event holding the account's state from before the upgrade.

## Deleting a trading account

Deleting the account returns only its native NEAR. First withdraw wNEAR, intents balances and any other tokens to the owner, and confirm they arrived ([withdrawal guide](https://docs.google.com/document/d/1runpneX6-h41beHh4k9FKtw3lKz6qgQbNzteR3RSCO4/)).

1. As the owner, add your own public key to the trading account:
   ```bash
   near contract call-function as-transaction $TA add_full_access_key json-args '{"public_key":"<your public key>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
2. Delete the trading account with that key, sending the remaining NEAR to the owner. It prompts for the key pair you added in step 1:
   ```bash
   near account delete-account $TA beneficiary $OWNER network-config testnet sign-with-plaintext-private-key send
   ```
