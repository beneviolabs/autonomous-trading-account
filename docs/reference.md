# Contract reference

The public methods of both contracts, with who may call them and what isn't obvious from the signature. The source is the authority: [`factory/src/lib.rs`](../contracts/factory/src/lib.rs) and [`trading-account/src/lib.rs`](../contracts/trading-account/src/lib.rs). Update this page in the same PR as any interface change.

## Factory

| Method | Caller | Notes |
|---|---|---|
| `new(owner_id, network, global_proxy_base58_hash)` | init | `network` is `"mainnet"` or `"testnet"` and picks the MPC signer; anything else panics. The factory still passes the signer to new trading accounts, which ignore it from 1.0.0. The hash is bs58 and is stored as raw 32 bytes, despite the argument name `global_proxy_base58_hash`. |
| `deposit_and_create_proxy_global(owner_id)` *payable* | `owner_id` itself | Creates `implicit_<24hex>.<factory>` with the global code and initializes it. The whole deposit becomes the trading account's balance. Refunds the deposit if creation fails. |
| `create_proxy_global(owner_id)` *payable* | `owner_id` itself | Same, but with no minimum deposit and no refund. Don't call it directly. |
| `get_base_account_name(owner_id)` | view | The `implicit_<24hex>` prefix for an owner. |
| `verify_implicit_base_name(owner_id, base_name)` | view | Checks that a prefix belongs to an owner. |
| `set_global_code_hash(code_hash_str)` | factory owner | bs58 hash. Trading accounts created afterwards get this code. Existing ones keep theirs, and `upgrade` can move them only to this hash. |
| `add_full_access_key(public_key)` | factory owner | Adds a key to the factory account. |
| `get_owner_id`, `get_signer_contract`, `get_proxy_code_base58_hash`, `get_proxy_code_hash_hex` | view | |

## Trading account

| Method | Caller | Notes |
|---|---|---|
| `new(owner_id)` | init (factory) | Ignores other arguments, such as the `signer_id` the factory still sends. |
| `add_agent(account_id)` | owner | Panics at 10 agents. |
| `remove_agent(account_id)` | owner | |
| `is_agent(account_id)` | view | True only for agents. The owner isn't one unless it adds itself. |
| `get_agents`, `get_owner_id` | view | |
| `contract_version` | view | `{"contract_version": "1.0.0", "state_version": 1}`: the versions compiled into the running code. |
| `migrate` | the account itself (*private*) | Brings older stored state up to the current state version. Runs after a code swap. Logs a `migrated` event with the pre-migration state and agents. |
| `upgrade(expected_hash)` *payable, exactly 1 yocto* | owner | Upgrades to the code hash the factory (the account's parent) points at. Fails unless the factory's hash equals `expected_hash`. Attach 100 Tgas. |
| `do_upgrade(expected_hash)` | the account itself (*private*) | `upgrade`'s callback: switches to the global contract and runs `migrate` in one receipt. |
| `swap(quote_payload, quote_signature)` *payable, exactly 1 yocto* | owner and agents | See below. |
| `swap_callback` | the account itself (*private*) | `swap`'s callback: fails the transaction if `mt_transfer` failed. |
| `add_full_access_key(public_key)` | owner | Adds one of the owner's own keys, for example before deleting the account. |
| `delete_key(public_key)` *payable, exactly 1 yocto* | owner | Deletes an access key from the trading account, such as the MPC key 1.0.0 no longer needs. |
| `add_full_access_key_and_register_with_intents(public_key)` *payable, exactly 1 yocto* | owner | Also registers the key on `intents.near`. That call fails on testnet, but the key is still added. |

### `swap`

Deposits into a live 1Click quote by calling `intents.near`'s `mt_transfer` from the trading account. The quote decides the transfer: it sends `amountIn` of `originAsset` to `depositAddress`. 1Click then swaps it and delivers the output to the quote's `recipient`, inside intents.

Attach exactly 1 yoctoNEAR, which is forwarded to `mt_transfer`, and at least 30 Tgas (`GAS_FOR_SWAP`). `mt_transfer` gets at least 10 Tgas plus all the gas `swap` doesn't use, so attaching more gives `intents.near` more. A swap burns about 6 Tgas in the sandbox, and the live `mt_transfer` used 4 Tgas; 100 Tgas leaves plenty of room.

| Argument | Notes |
|---|---|
| `quote_payload` | The exact JSON 1Click signed: the flattened, key-sorted object 1Click's SDK signs (Peerfolio Backend's `build_signed_payload`). It must be byte for byte what was signed. |
| `quote_signature` | The quote response's `signature`, `ed25519:<base58>`. |

The contract checks that 1Click's compiled-in key signed the payload, then the terms in the [security model](trading-account.md#security-model). It doesn't check values such as `amountIn`: once the signature verifies, they're 1Click's. A quote can be used again while its deadline hasn't passed; each use is another deposit to the same address.

Common failures:

| Symptom | Cause |
|---|---|
| `Requires attached deposit of exactly 1 yoctoNEAR` | The deposit isn't 1 yocto. |
| `Unauthorized: only the owner or an agent can swap` | The caller is neither. |
| `Not enough gas attached. Please attach at least 30 TGas` | Less than `GAS_FOR_SWAP` was attached. |
| `Invalid quote signature: …` | The quote isn't signed by 1Click's key, or `quote_payload` isn't byte for byte what 1Click signed. |
| `Invalid quote payload: …` | The payload lacks a field the contract reads, has one of the wrong type, or repeats a key. |
| `Quote is a dry quote, which has no deposit address` | A dry quote. Request a live one. |
| `Quote has no deadline`, `Quote deadline … is not an ISO 8601 date-time`, `Quote deadline … has passed` | The deadline is missing, unreadable, or not after the block time. A deadline without a time zone is read as UTC. |
| `Quote … is not the trading account or its owner`, `Quote … must be INTENTS`, `Quote … is not allowed`, `… above the 200 bps maximum` | The quote pays out or refunds outside the user's accounts, or asks for too much slippage. |
| `swap: mt_transfer on intents.near failed` | `intents.near` rejected the transfer, for example because the trading account's balance of `originAsset` is short. Nothing moved. |
