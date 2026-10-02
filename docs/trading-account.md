# Trading account

The per-user contract, created by the [factory](factory.md) at `implicit_<24hex>.auth.peerfolio.near`. The owner moves in only the funds an agent may trade with. Authorized users can then have the MPC signer sign transactions from the trading account, limited to an allowlist. Method details are in the [reference](reference.md#trading-account).

## Security model

- The MPC key is derived from the trading account and a derivation path, so only the trading account contract can get signatures for it. The key has full access to the trading account.
- The contract signs only transactions to allowlisted contracts and methods, defined in [`actions.rs`](../contracts/trading-account/src/actions.rs):
  - contracts: `wrap.near`, `intents.near`, `wrap.testnet`
  - methods: `add_public_key`, `ft_transfer_call`, `near_deposit`, `mt_transfer_call`, `mt_transfer`, `ft_withdraw`
- **Arguments aren't checked.** Every allowed method is allowed on every allowed contract. An authorized user can call `ft_withdraw` or `mt_transfer` on `intents.near` with any recipient. Only authorize accounts you trust with the trading account's funds.
- An authorized user can also change the MPC signer contract with `set_signer_id`.
- **The owner isn't an authorized user** unless it adds itself, so it can't call `request_signature` by default.
- **The trading account never broadcasts anything.** It returns a signed transaction, and the caller broadcasts it. The transaction runs from the trading account, so gas and attached deposits come out of its balance.
- The allowlist is compiled in. Changing it means [releasing new trading account code](releases.md#release-trading-account-code), and existing trading accounts keep their old code.

## Lifecycle

These commands use testnet and near-cli-rs (tested with 0.22). Variables:
- `$OWNER`: a funded NEAR implicit account. To make one, run `near account create-account fund-later use-auto-generation save-to-folder <dir>` and send NEAR to the 64-hex ID it prints ([faucet](https://near-faucet.io/)).
- `$OWNER_KEY`: the owner's key file, `<dir>/<id>.json`. Commands signed by the owner use `sign-with-access-key-file $OWNER_KEY`.
- `$AGENT`: the authorized user. Peerfolio's mainnet agent is `bot.peerfolio.near`. There's no live testnet agent, so use any testnet account you control.
- `$TA`: the trading account ID.

1. **Create** the trading account. The owner must sign. Look up its name first:
   ```bash
   near contract call-function as-read-only auth.peerfolio.testnet get_base_account_name json-args "{\"owner_id\":\"$OWNER\"}" network-config testnet now
   ```
   ```bash
   near contract call-function as-transaction auth.peerfolio.testnet deposit_and_create_proxy_global json-args "{\"owner_id\":\"$OWNER\"}" prepaid-gas '300.0 Tgas' attached-deposit '0.12 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
   `$TA` is `<name>.auth.peerfolio.testnet`. Store it; don't re-derive it later.
2. **Derive the MPC key.** By convention, the derivation path is the trading account ID.
   ```bash
   near contract call-function as-read-only v1.signer-prod.testnet derived_public_key json-args "{\"path\":\"$TA\",\"predecessor\":\"$TA\",\"domain_id\":0}" network-config testnet now
   ```
3. **Register the key and authorize the agent**, as the owner:
   ```bash
   near contract call-function as-transaction $TA add_full_access_key json-args '{"public_key":"secp256k1:<MPC key>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
   ```bash
   near contract call-function as-transaction $TA add_authorized_user json-args "{\"account_id\":\"$AGENT\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as $OWNER network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
4. **Fund it** with what the agent may trade. Keep some NEAR there at all times, because it pays gas for every signed transaction.
   ```bash
   near tokens $OWNER send-near $TA '0.2 NEAR' network-config testnet sign-with-access-key-file $OWNER_KEY send
   ```
5. **Trade.** The agent requests a signed transaction and broadcasts it:

   ```mermaid
   sequenceDiagram
       participant Agent as Agent
       participant TA as Trading account
       participant MPC as MPC signer
       participant NEAR as NEAR

       Agent->>NEAR: MPC key nonce + recent block hash
       Agent->>TA: request_signature(contract_id, actions_json, nonce, block_hash, mpc_signer_pk, derivation_path)
       TA->>TA: check caller and allowlist, build transaction
       TA->>MPC: sign(transaction hash, path)
       MPC-->>TA: signature
       TA-->>Agent: base64 signed transaction
       Agent->>NEAR: broadcast (runs from the trading account)
   ```

   1. Get the MPC key's current nonce from `near account list-keys $TA network-config testnet now`, and a recent block hash, for example from [testnet.nearblocks.io](https://testnet.nearblocks.io).
   2. Request the signature. This one wraps 0.05 NEAR. `actions_json` is a JSON *string*, and `nonce` is the current nonce + 1.
      ```bash
      near contract call-function as-transaction $TA request_signature json-args "{\"contract_id\":\"wrap.testnet\",\"actions_json\":\"[{\\\"type\\\":\\\"FunctionCall\\\",\\\"method_name\\\":\\\"near_deposit\\\",\\\"args\\\":{},\\\"gas\\\":\\\"30000000000000\\\",\\\"deposit\\\":\\\"50000000000000000000000\\\"}]\",\"nonce\":\"<nonce + 1>\",\"block_hash\":\"<block hash>\",\"mpc_signer_pk\":\"secp256k1:<MPC key>\",\"derivation_path\":\"$TA\"}" prepaid-gas '300.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $AGENT network-config testnet sign-with-keychain send
      ```
   3. Broadcast the base64 value it returns:
      ```bash
      near transaction send-signed-transaction '<base64>' network-config testnet
      ```

   If something fails, see the [common failures](reference.md#request_signature).

Example transactions, from before the implicit-owner naming:
- Testnet: [wrapping 1 NEAR](https://testnet.nearblocks.io/txns/Hi2pfe89tBdMN2oY2dFXLuHcSBVFotx6pHViDQuKUZDi), signed through [this `request_signature` call](https://testnet.nearblocks.io/txns/831u2KqbdtzvJti5HUhGnp4tZD7Q8onUzD11rwBjrAAm) (older API).
- Mainnet: [adding a public key on `intents.near`](https://nearblocks.io/txns/GRw6oEWjAQ2QT9oDtsgBSRWr3s4oCW4A8zCpHCRXD62s), signed through [this request](https://nearblocks.io/txns/9PJXbvcb4RMxjwK8VW4N54RnvrjENUCr6N1nv9f3DZJQ).
- Onboarding: [creating a trading account](https://testnet.nearblocks.io/txns/CF1ainGjroxtppNTWWkFgQsiC5kC4iJ3X7v8FgLMrWDE?tab=execution), [registering the MPC key](https://testnet.nearblocks.io/txns/5Jyn459DhAaRxEqvTo3x724cgxCpjTH1Jaoc7uyVNQt9).

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
