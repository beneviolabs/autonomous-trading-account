# Builds and releases

How both contracts are built, and how new code reaches testnet and mainnet. Accounts and owners are in the [README](../README.md#deployments).

## Builds

| | How | Deploy it? |
|---|---|---|
| Release | The `contract-wasm` artifact from CI, or `make release` locally | Yes. Only deploy these. |
| Dev | `make build`, `contracts/*/build.sh` | No. Tests only. |

Both write to `contracts/target/near/<crate>/<crate>.wasm`:
- trading account: `contracts/target/near/trading_account/trading_account.wasm`
- factory: `contracts/target/near/trading_account_factory/trading_account_factory.wasm`

- **Release builds are reproducible.** They run `cargo near build reproducible-wasm` in NEAR's build image, pinned by digest under `[package.metadata.near.reproducible_build]` in each crate's `Cargo.toml`. The same commit gives the same hash on any machine. The wasm also records the repository, commit and build image (NEP-330), so explorers can verify it against the source.
- **Get release wasm from CI.** The `contracts` workflow runs `make release`, prints each wasm's SHA-256 (hex and bs58) in its log, and uploads both wasms as the `contract-wasm` artifact, kept for 90 days.
  - CI uploads the artifact only for pushes to `main` and for manual runs. Pull request and other branch runs build and test but don't upload.
  - Mainnet releases come from `main`. A manual run on a branch (`gh workflow run contracts.yml --ref <branch>`) is for testnet rehearsals only.
  - CI runs on pull requests and on pushes to `main` that touch contract files. Otherwise, start a run with `gh workflow run contracts.yml --ref main`.
- **`make release` locally** needs Docker and a clean tree with everything committed, including `Cargo.lock`. NEAR's build images are x86-only, so on Apple Silicon Docker emulates them and the build is slow. Use it to check a CI hash independently, not as the usual route.
- **To change the build image**, update `image` and `image_digest` in both crates together. That changes both hashes.
- **Dev builds aren't reproducible.** They compile natively, and the same commit produces different code on macOS arm64 and on Linux x86_64.

### Rust version

The toolchain is Rust 1.97.1, in `rust-toolchain.toml` and in the release image (`sourcescan/cargo-near:0.22.0-rust-1.97.1`) in each crate's `Cargo.toml`. Change them together, and pick a Rust version that has a [release image](https://hub.docker.com/r/sourcescan/cargo-near/tags).

- Rust 1.87 and later emit bulk-memory and non-trapping float-to-int wasm instructions. NEAR accepts them from protocol 84 (nearcore 2.12), and mainnet and testnet are past that.
- cargo-near only allows Rust above 1.86 when near-sdk declares protocol 84 or later as its minimum. near-sdk 5.28+ does, so **don't go back below near-sdk 5.28** without also going back to Rust 1.86.
- The test sandbox is neard 2.13.4, via `near-workspaces` 0.23.

### What's deployed now

Checked 2026-10-01:

| | Hash | Built from |
|---|---|---|
| Trading account global code | `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f` (hex `59136c3b557222ed2a30f8d02953ab50633566a49c4e107e2314f3a72c19b1f8`) | A native macOS build from before the repo reorg. It can't be reproduced, so the release-gate tests use a copy fetched from mainnet, `contracts/trading-account/res/v0.wasm`. |
| Factory code, mainnet and testnet | hex `670ecb8001989c5dd36d8ff96f45bef138f3bac417134c67ac615768c57c4b40` | The old Docker CI build of `a26cd2f` (PR #166) |

The next release build will produce new hashes for both, even for unchanged code, because crate names, paths, the lockfile and the build image all changed in the reorg.

## Which release do I need?

| What changed | Procedure | Affects |
|---|---|---|
| Trading account code (`contracts/trading-account`) | [Release trading account code](#release-trading-account-code) | Trading accounts created afterwards. The factory code isn't touched. |
| Factory code (`contracts/factory`) | [Release factory code](#release-factory-code) | The factory only. It keeps its owner, MPC signer and trading account code hash. |
| A new factory account is needed | [Deploy a new factory](factory.md#deploy-a-new-factory) | A new factory. Accounts created by the old one stay where they are. |

Existing trading accounts are never changed by a release. They keep the code they were created with.

Who's involved on mainnet: the factory owner is the DAO `peerfolio.sputnik-dao.near`, so every change to `auth.peerfolio.near` is a [DAO proposal](#dao-proposals). Proposals need 3 of 4 council approvals, and council members sign with their Ledger. On testnet, the factory owner `peerfolio.peerfolio.testnet` and the factory account `auth.peerfolio.testnet` sign directly from the keychain.

## Get the release build

Step 1 of both releases.

1. Find the CI run for the commit you're releasing. Mainnet releases use a commit on `main`, from a run whose event is `push` or `workflow_dispatch`; check that it succeeded.
   ```bash
   export COMMIT=<full release commit SHA>
   gh run list --workflow contracts.yml --commit $COMMIT --json databaseId,headSha,event,conclusion
   ```
   If there's none, start one. A manual run builds the head of the ref you give it, so give it a ref that points at `$COMMIT`: `--ref main` if `$COMMIT` is `main`'s head, otherwise a tag (`git tag release-<name> $COMMIT && git push origin release-<name>`, then `--ref release-<name>`). For a testnet rehearsal of unmerged code, use the branch. Before downloading, check the run's `headSha` is `$COMMIT`:
   ```bash
   gh run view <run id> --json headSha,event,conclusion
   ```
2. Download the wasms in place of any local builds:
   ```bash
   rm -rf contracts/target/near
   gh run download <run id> -n contract-wasm -D contracts/target/near
   ```
3. Record the hashes CI printed, and check the downloaded files match the hex ones:
   ```bash
   gh run view <run id> --log | grep "SHA-256 checksum"
   shasum -a 256 contracts/target/near/*/*.wasm
   ```

To check a hash independently, run `make release` on a clean checkout of the commit; it should print the same hash.

## DAO proposals

Each mainnet change to the factory is one function call on `auth.peerfolio.near`, proposed to the DAO. The release steps give the method and its arguments.

1. Build the proposal. Set your own council account and Ledger path, and the method and arguments from the release step.
   ```bash
   export COUNCIL=<your council account>
   export HD_PATH="m/44'/397'/0'/0'/1'"
   export METHOD=<method>
   export ARGS_JSON='<arguments>'
   export ARGS=$(echo -n "$ARGS_JSON" | base64 | tr -d '\n')
   export KIND='{"FunctionCall":{"receiver_id":"auth.peerfolio.near","actions":[{"method_name":"'$METHOD'","args":"'$ARGS'","deposit":"0","gas":"15000000000000"}]}}'
   ```
2. Submit it. The 0.1 NEAR is the proposal bond. The call returns the proposal ID.
   ```bash
   near contract call-function as-transaction peerfolio.sputnik-dao.near add_proposal json-args "{\"proposal\":{\"description\":\"<what and why, with the commit and hash>\",\"kind\":$KIND}}" prepaid-gas '30.0 Tgas' attached-deposit '0.1 NEAR' sign-as $COUNCIL network-config mainnet sign-with-ledger --seed-phrase-hd-path "$HD_PATH" send
   ```
3. Check it:
   ```bash
   near contract call-function as-read-only peerfolio.sputnik-dao.near get_proposal json-args '{"id":<ID>}' network-config mainnet now
   ```
4. Each council member votes. `KIND` must be exactly what was submitted, so share `METHOD` and `ARGS_JSON` with the other voters; they set `ARGS` and `KIND` from them as in step 1.
   ```bash
   near contract call-function as-transaction peerfolio.sputnik-dao.near act_proposal json-args "{\"id\":<ID>,\"action\":\"VoteApprove\",\"proposal\":$KIND}" prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as $COUNCIL network-config mainnet sign-with-ledger --seed-phrase-hd-path "$HD_PATH" send
   ```
   The call runs when the third approval comes in.

These commands are adapted from the ft-core deploy log of 2026-09-29 and haven't been run as written yet.

## Release trading account code

New trading account code goes out as a new global contract, and the factory is then pointed at its hash. The factory code itself isn't redeployed.

Anyone with a funded account can deploy the global contract. On mainnet that's `peerfolio.near`, and it costs about 40 NEAR for storage; arrange funding or reimbursement first.

### Step 1: Get the release build

See [Get the release build](#get-the-release-build). Record the commit and the trading account's bs58 hash.

### Step 2: Rehearse on testnet

1. Deploy the global contract. The bs58 hash it prints should match step 1.
   ```bash
   near contract deploy-as-global use-file contracts/target/near/trading_account/trading_account.wasm as-global-hash peerfolio.testnet network-config testnet sign-with-keychain send
   ```
2. Point the factory at it:
   ```bash
   near contract call-function as-transaction auth.peerfolio.testnet set_global_code_hash json-args '{"code_hash_str":"<bs58 hash>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as peerfolio.peerfolio.testnet network-config testnet sign-with-keychain send
   ```
3. Verify:
   1. `get_proxy_code_base58_hash` returns the new hash:
      ```bash
      near contract call-function as-read-only auth.peerfolio.testnet get_proxy_code_base58_hash json-args '{}' network-config testnet now
      ```
   2. Create a new trading account ([lifecycle](trading-account.md#lifecycle) step 1).
   3. `near account view-account-summary $TA network-config testnet now` shows the new hash, in hex, under `Global Contract`.
   4. Run through the rest of the [lifecycle](trading-account.md#lifecycle).

### Step 3: Deploy the global contract on mainnet

```bash
near contract deploy-as-global use-file contracts/target/near/trading_account/trading_account.wasm as-global-hash peerfolio.near network-config mainnet sign-with-keychain send
```

### Step 4: Point the mainnet factory at it

Run a [DAO proposal](#dao-proposals) with:

```bash
export METHOD=set_global_code_hash
export ARGS_JSON='{"code_hash_str":"<bs58 hash>"}'
```

### Step 5: Verify on mainnet

```bash
near contract call-function as-read-only auth.peerfolio.near get_proxy_code_base58_hash json-args '{}' network-config mainnet now
```

The next trading account created should show the new hex hash under `Global Contract` in `near account view-account-summary`. Then update the hash in the [README](../README.md#deployments) and in [What's deployed now](#whats-deployed-now).

## Release factory code

Use this for changes like bug fixes, gas tweaks and new view methods. A code-only redeploy keeps the factory's stored state. It only works if the new code reads the same stored fields, so if the `TradingAccountFactory` struct changed, [deploy a new factory](factory.md#deploy-a-new-factory) instead.

`auth.peerfolio.near` has no access keys, so on mainnet a council member adds a temporary key through the DAO, deploys with it, and then deletes it. On testnet, the `auth.peerfolio.testnet` key is in the keychain.

> **TODO: deploy factory code without a temporary key.** For the length of a release, one council member alone holds a full-access key to the factory, outside the DAO's 3-of-4 approval, and nothing enforces that it's deleted afterwards. Instead, the factory could have an owner-only method that deploys the wasm passed to it onto its own account. Sputnik DAO already supports this: `store_blob` uploads the wasm to the DAO, and an `UpgradeRemote` proposal calls that method with it once approved. The factory would never need an access key. That's a factory code change, so it needs review, and the first release that adds the method still has to go out this way.

### Step 1: Get the release build

See [Get the release build](#get-the-release-build). Record the commit and the factory's hex hash.

### Step 2: Rehearse on testnet

1. Deploy without calling `new`:
   ```bash
   near contract deploy auth.peerfolio.testnet use-file contracts/target/near/trading_account_factory/trading_account_factory.wasm without-init-call network-config testnet sign-with-keychain send
   ```
2. Check the result:
   - `near account view-account-summary auth.peerfolio.testnet network-config testnet now` shows the hex hash from step 1.
   - These still return what they did before: `get_owner_id`, `get_signer_contract` and `get_proxy_code_base58_hash`.
   ```bash
   near contract call-function as-read-only auth.peerfolio.testnet get_owner_id json-args '{}' network-config testnet now
   ```
3. Create a trading account to check the new factory code works ([lifecycle](trading-account.md#lifecycle) step 1).

### Step 3: Add a temporary key to the mainnet factory

1. Generate the key outside the repo:
   ```bash
   export KEY_DIR=$(mktemp -d)
   near account create-account fund-later use-auto-generation save-to-folder $KEY_DIR
   export KEY_FILE=$(ls $KEY_DIR/*.json)
   export PUBKEY=$(jq -r .public_key $KEY_FILE)
   ```
2. Run a [DAO proposal](#dao-proposals) with:
   ```bash
   export METHOD=add_full_access_key
   export ARGS_JSON="{\"public_key\":\"$PUBKEY\"}"
   ```
3. After the third approval, check the key is on the factory:
   ```bash
   near account list-keys auth.peerfolio.near network-config mainnet now
   ```

### Step 4: Deploy on mainnet

```bash
near contract deploy auth.peerfolio.near use-file contracts/target/near/trading_account_factory/trading_account_factory.wasm without-init-call network-config mainnet sign-with-access-key-file $KEY_FILE send
```

Then run the same checks as step 2.2 with `auth.peerfolio.near` and `network-config mainnet`, and update [What's deployed now](#whats-deployed-now).

### Step 5: Delete the temporary key

Don't skip this. The factory should have no access keys.

```bash
near account delete-keys auth.peerfolio.near public-keys $PUBKEY network-config mainnet sign-with-access-key-file $KEY_FILE send
rm -rf $KEY_DIR
near account list-keys auth.peerfolio.near network-config mainnet now
```
