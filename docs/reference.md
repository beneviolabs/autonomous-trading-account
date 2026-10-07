# Contract reference

The public methods of both contracts, with who may call them and what isn't obvious from the signature. The source is the authority: [`factory/src/lib.rs`](../contracts/factory/src/lib.rs) and [`trading-account/src/lib.rs`](../contracts/trading-account/src/lib.rs). Update this page in the same PR as any interface change.

## Factory

| Method | Caller | Notes |
|---|---|---|
| `new(owner_id, network, global_proxy_base58_hash)` | init | `network` is `"mainnet"` or `"testnet"` and picks the MPC signer; anything else panics. The hash is bs58 and is stored as raw 32 bytes, despite the argument name `global_proxy_base58_hash`. |
| `deposit_and_create_proxy_global(owner_id)` *payable* | `owner_id` itself | Creates `implicit_<24hex>.<factory>` with the global code and initializes it. The whole deposit becomes the trading account's balance. Refunds the deposit if creation fails. |
| `create_proxy_global(owner_id)` *payable* | `owner_id` itself | Same, but with no minimum deposit and no refund. Don't call it directly. |
| `get_base_account_name(owner_id)` | view | The `implicit_<24hex>` prefix for an owner. |
| `verify_implicit_base_name(owner_id, base_name)` | view | Checks that a prefix belongs to an owner. |
| `set_global_code_hash(code_hash_str)` | factory owner | bs58 hash. Affects trading accounts created afterwards only. |
| `add_full_access_key(public_key)` | factory owner | Adds a key to the factory account. |
| `get_owner_id`, `get_signer_contract`, `get_proxy_code_base58_hash`, `get_proxy_code_hash_hex` | view | |

## Trading account

| Method | Caller | Notes |
|---|---|---|
| `new(owner_id, signer_id)` | init (factory) | |
| `add_agent(account_id)` | owner | Panics at 10 agents. |
| `remove_agent(account_id)` | owner | |
| `is_agent(account_id)` | view | True only for agents. The owner isn't one unless it adds itself. |
| `get_agents`, `get_owner_id`, `get_signer_id` | view | |
| `set_signer_id(signer_id)` | owner | Changes the MPC signer contract. |
| `request_signature(...)` *payable* | agents | See below. |
| `add_full_access_key(public_key)` | owner | Registers the MPC key, or the owner's own key before deleting the account. |
| `delete_key(public_key)` *payable, exactly 1 yocto* | owner | Deletes an access key from the trading account. Deleting the MPC key invalidates every transaction it signed that hasn't been broadcast yet. |
| `add_full_access_key_and_register_with_intents(public_key)` *payable, exactly 1 yocto* | owner | Also registers the key on `intents.near`. That call fails on testnet, but the key is still added. |

### `request_signature`

Attach at least 100 Tgas (300 recommended) and 1 yoctoNEAR, which is forwarded to the MPC signer.

| Argument | Notes |
|---|---|
| `contract_id` | Receiver of the signed transaction. Must be `intents.near`. |
| `actions_json` | A JSON **string** holding a non-empty array of `mt_transfer` calls, e.g. `[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]`. `gas` and `deposit` are strings. |
| `nonce` | Greater than the MPC key's current nonce on the trading account, and unique per pending transaction. |
| `block_hash` | A recent block hash. The transaction expires about 24h later. |
| `mpc_signer_pk` | The MPC key, `secp256k1:…`. The transaction is signed for this key, and the callback rejects a signature that doesn't recover to it. |
| `derivation_path` | The path used to derive `mpc_signer_pk`. By convention, the trading account ID. |
| `domain_id` | Optional. Only `0` (secp256k1) works. |

It returns a base64 borsh `SignedTransaction` from the trading account, also logged as `Signed transaction (base64): …`. The caller must broadcast it. Gas and attached deposits come out of the trading account's balance.

Common failures:

| Symptom | Cause |
|---|---|
| `Failed to deserialize input from JSON … invalid character '<'` | A `<placeholder>` from the docs was left in the arguments. |
| `Unauthorized: only agents can request signatures` | The caller isn't an agent. |
| `<method> on <contract> is not allowed` | The call isn't `mt_transfer` on `intents.near`, the only one the allowlist permits. |
| `unknown variant` | An action other than `FunctionCall`, such as a bare `Transfer`. |
| `Invalid signature: recovered key doesn't match the transaction's public key` | `mpc_signer_pk` isn't the key derived from `derivation_path`, or the signer contract signed with another key. |
| Broadcast rejected: access key not found | The MPC key isn't on the trading account. |
| Broadcast rejected: invalid nonce | The nonce was already used. Request again with a higher one. |
| Broadcast rejected: not enough balance | The trading account can't cover gas or the attached deposit. |
