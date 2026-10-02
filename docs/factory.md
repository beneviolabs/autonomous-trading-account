# Factory

The factory creates trading accounts as sub-accounts of itself. Each one runs the trading account code that was deployed once as a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). The factory stores the code's hash and gives it to every new account. That way a new account costs about 0.001 NEAR instead of about 3.8 NEAR to store its own copy of the wasm. Accounts and owners are listed in the [README](../README.md#deployments), and methods in the [reference](reference.md#factory).

## Trading account naming

A trading account is named `implicit_<24hex>.<factory>`. `<24hex>` is the first 24 hex characters of `sha256(first 32 characters of owner_id)`. Get it from `get_base_account_name`.

- **Owners must be NEAR implicit accounts** (64 lowercase hex characters). Named accounts and `0x…` accounts are rejected.
- **Only the owner can create its own trading account.** The caller must be `owner_id`.
- **The factory account ID can be at most 30 characters**, so trading account IDs fit NEAR's 64-character limit.
- **Attach about 0.12 NEAR** when creating one. The contract minimum is far lower, but the deposit becomes the trading account's balance, and that balance pays for gas.

## Builds

| | Command | Deploy it? |
|---|---|---|
| Release | `make release` | Yes. Only deploy these. |
| Dev | `make build`, `contracts/*/build.sh` | No |

Both write `contracts/target/near/<crate>/<crate>.wasm`, for example `trading_account/trading_account.wasm`.

- **Release builds are reproducible.** They run `cargo near build reproducible-wasm` in NEAR's build image, pinned by digest in each crate's `Cargo.toml`. The same commit gives the same hash on any machine, and the wasm records its source commit so explorers can verify it. They need Docker and a clean, committed tree. To change the image, update both crates together.
- **Dev builds aren't.** They compile natively, so the same commit produces different code on macOS and on Linux.
- **The live global hash `6ziTqYXT…` can't be reproduced.** It came from a native build before this repo's layout changed. The next release will have new hashes for both contracts even if the code is unchanged.
- Rust 1.87 and later produce wasm that near-sandbox rejects. The toolchain and the release image both use 1.86.

## Releases

The two contracts are released independently:

- **New trading account code** needs a global deploy and a factory call. The factory code stays as it is.
- **New factory code** is a redeploy of the factory account. The factory's stored owner, MPC signer and global hash are kept, so new trading accounts still get the same code.

Neither changes existing trading accounts. They keep the code they were created with.

Every release starts with `make release` on a clean checkout of the release commit. Record the commit and the hashes it prints. Rehearse on testnet before mainnet.

On mainnet the factory owner is the DAO `peerfolio.sputnik-dao.near`, and `auth.peerfolio.near` has no access keys. Any change goes through a DAO proposal (below). On testnet the owner `peerfolio.peerfolio.testnet` calls the factory directly, and the factory account has its own key.

### DAO proposals

Each mainnet change is a Function Call proposal to `auth.peerfolio.near`. Council members sign with their Ledger. Set `METHOD` and `ARGS` as given in each release step, then:

```bash
export COUNCIL=<your council account>
export HD_PATH="m/44'/397'/0'/0'/1'"   # your Ledger path
export KIND='{"FunctionCall":{"receiver_id":"auth.peerfolio.near","actions":[{"method_name":"'$METHOD'","args":"'$ARGS'","deposit":"0","gas":"15000000000000"}]}}'
```

1. Submit the proposal. The 0.1 NEAR is the proposal bond. The call returns the proposal ID.
   ```bash
   near contract call-function as-transaction peerfolio.sputnik-dao.near add_proposal json-args "{\"proposal\":{\"description\":\"<what and why, commit, hash>\",\"kind\":$KIND}}" prepaid-gas '30.0 Tgas' attached-deposit '0.1 NEAR' sign-as $COUNCIL network-config mainnet sign-with-ledger --seed-phrase-hd-path "$HD_PATH" send
   ```
2. Check it:
   ```bash
   near contract call-function as-read-only peerfolio.sputnik-dao.near get_proposal json-args '{"id":<ID>}' network-config mainnet now
   ```
3. Each council member votes, with `KIND` set exactly as it was for the submission. The proposal runs once 3 of 4 members have approved it.
   ```bash
   near contract call-function as-transaction peerfolio.sputnik-dao.near act_proposal json-args "{\"id\":<ID>,\"action\":\"VoteApprove\",\"proposal\":$KIND}" prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as $COUNCIL network-config mainnet sign-with-ledger --seed-phrase-hd-path "$HD_PATH" send
   ```

### Release trading account code

1. Deploy the wasm as a global contract. Any account can pay. On mainnet it costs about 40 NEAR. Record the bs58 hash it prints.
   ```bash
   near contract deploy-as-global use-file contracts/target/near/trading_account/trading_account.wasm as-global-hash peerfolio.near network-config mainnet sign-with-keychain send
   ```
2. Point the factory at the new hash.
   - Mainnet: a [DAO proposal](#dao-proposals) with
     ```bash
     export METHOD=set_global_code_hash
     export ARGS=$(echo -n '{"code_hash_str":"<bs58 hash>"}' | base64 | tr -d '\n')
     ```
   - Testnet:
     ```bash
     near contract call-function as-transaction auth.peerfolio.testnet set_global_code_hash json-args '{"code_hash_str":"<bs58 hash>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as peerfolio.peerfolio.testnet network-config testnet sign-with-keychain send
     ```
3. Verify that the factory returns the new hash:
   ```bash
   near contract call-function as-read-only auth.peerfolio.near get_proxy_code_base58_hash json-args '{}' network-config mainnet now
   ```
   A trading account created afterwards shows the same hash, in hex, under `Global Contract` in `near account view-account-summary <account> network-config mainnet now`. Compare it with `get_proxy_code_hash_hex`.

### Release factory code

A code-only redeploy keeps the factory's stored state. It only works if the new code reads the same stored fields. Mainnet steps:

1. Generate a temporary key outside the repo:
   ```bash
   export KEY_DIR=$(mktemp -d)
   near account create-account fund-later use-auto-generation save-to-folder $KEY_DIR
   export KEY_FILE=$(ls $KEY_DIR/*.json)
   export PUBKEY=$(jq -r .public_key $KEY_FILE)
   ```
2. Pass a [DAO proposal](#dao-proposals) with
   ```bash
   export METHOD=add_full_access_key
   export ARGS=$(echo -n "{\"public_key\":\"$PUBKEY\"}" | base64 | tr -d '\n')
   ```
   When it has run, check that the key is on the factory:
   ```bash
   near account list-keys auth.peerfolio.near network-config mainnet now
   ```
3. Deploy without calling `new`:
   ```bash
   near contract deploy auth.peerfolio.near use-file contracts/target/near/trading_account_factory/trading_account_factory.wasm without-init-call network-config mainnet sign-with-access-key-file $KEY_FILE send
   ```
4. Check that the summary shows the factory hex hash from `make release`, and that `get_owner_id`, `get_signer_contract` and `get_proxy_code_base58_hash` return the same values as before:
   ```bash
   near account view-account-summary auth.peerfolio.near network-config mainnet now
   ```
5. Delete the temporary key and remove its file:
   ```bash
   near account delete-keys auth.peerfolio.near public-keys $PUBKEY network-config mainnet sign-with-access-key-file $KEY_FILE send
   rm -rf $KEY_DIR
   ```

On testnet the factory account has its own key in the keychain, so only steps 3 and 4 apply:

```bash
near contract deploy auth.peerfolio.testnet use-file contracts/target/near/trading_account_factory/trading_account_factory.wasm without-init-call network-config testnet sign-with-keychain send
```

### Deploy a new factory

[`contracts/factory/deploy.sh`](../contracts/factory/deploy.sh) creates `auth.peerfolio.<near|testnet>` from `peerfolio.<near|testnet>` with 4 NEAR, then deploys the factory and calls `new`:

```bash
contracts/factory/deploy.sh peerfolio.sputnik-dao.near <bs58 global hash> mainnet
```

- `peerfolio.<near|testnet>`'s key has to be in your keychain. If that account uses a Ledger, temporarily add a keychain full-access key and delete it afterwards.
- If the factory account already exists, the script only redeploys the code and ignores the owner and hash arguments.
- When it finishes, it checks that the deployed code hash matches the local wasm.
- Afterwards, update `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*` in ft-core.
