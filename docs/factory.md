# Factory

The factory creates trading accounts as sub-accounts of itself. Each one runs the trading account code that was deployed once as a [NEP-591 global contract](https://github.com/near/NEPs/blob/master/neps/nep-0591.md). The factory stores the code's hash and gives it to every new account. That way a new account costs about 0.001 NEAR instead of about 3.8 NEAR to store its own copy of the wasm. Accounts and owners are listed in the [README](../README.md#deployments), and methods in the [reference](reference.md#factory).

## Trading account naming

A trading account is named `implicit_<24hex>.<factory>`, for example `implicit_07454f3217b9229ead97798c.auth.peerfolio.near`. `<24hex>` is the first 24 hex characters of `sha256(first 32 characters of owner_id)`. This is the original derivation, kept so existing users keep their names. Get it from `get_base_account_name` rather than computing it yourself. Once the account exists, store its ID instead of re-deriving it.

- **Owners must be NEAR implicit accounts** (64 lowercase hex characters). Named accounts (`alice.near`), `0x…` accounts and uppercase hex are rejected.
- **Only the owner can create its own trading account.** The caller must be `owner_id`.
- **The factory account ID can be at most 30 characters**, so `implicit_` + 24 hex + `.` + the factory ID fits NEAR's 64-character limit.
- **Attach about 0.12 NEAR** when creating one. The contract minimum (1,000,000 yoctoNEAR) is a placeholder. The deposit becomes the trading account's balance, and that balance pays gas for every signed transaction. Mainnet onboarding attaches 0.04 to 0.12 NEAR.
- If creation fails (for example, the account already exists), nothing is created and the deposit is refunded.

These rules came from a security review. Named owners used to be mapped by stripping `.near`/`.testnet` and replacing dots with hyphens. That let distinct owners collide (`alice.near` and `alice.testnet`), and let anyone create an account on someone else's behalf. The collision analysis is in the doc comment on `get_base_account_name`.

## Builds

Both contracts are built the same way.

| | How | Deploy it? |
|---|---|---|
| Release | The `contract-wasm` artifact from CI, or `make release` locally | Yes. Only deploy these. |
| Dev | `make build`, `contracts/*/build.sh` | No. Tests only. |

Both write to `contracts/target/near/<crate>/<crate>.wasm`:
- trading account: `contracts/target/near/trading_account/trading_account.wasm`
- factory: `contracts/target/near/trading_account_factory/trading_account_factory.wasm`

- **Release builds are reproducible.** They run `cargo near build reproducible-wasm` in NEAR's build image, pinned by digest under `[package.metadata.near.reproducible_build]` in each crate's `Cargo.toml`. The same commit gives the same hash on any machine. The wasm also records the repository, commit and build image (NEP-330), so explorers can verify it against the source.
- **Get release wasm from CI.** The `contracts` workflow runs `make release`, prints each wasm's SHA-256 (hex and bs58) in its log, and uploads both wasms as the `contract-wasm` artifact, kept for 90 days. The commands are in step 1 of each release procedure.
  - CI uploads the artifact only for pushes to `main` and for manual runs. Pull request and other branch runs build and test but don't upload.
  - Mainnet releases come from `main`. A manual run on a branch (`gh workflow run contracts.yml --ref <branch>`) is for testnet rehearsals only.
  - CI only runs on pushes that touch contract files. Otherwise, start a run with `gh workflow run contracts.yml --ref main`.
- **`make release` locally** needs Docker and a clean tree with everything committed, including `Cargo.lock`. NEAR's build images are x86-only, so on Apple Silicon Docker emulates them and the build is slow. Use it to check a CI hash independently, not as the usual route.
- **To change the build image**, update `image` and `image_digest` in both crates together. That changes both hashes.
- **Dev builds aren't reproducible.** They compile natively, and the same commit produces different code on macOS arm64 and on Linux x86_64.
- **Rust version:** see [below](#rust-version).

What's deployed now (checked 2026-10-01):

| | Hash | Built from |
|---|---|---|
| Trading account global code | `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f` (hex `59136c3b557222ed2a30f8d02953ab50633566a49c4e107e2314f3a72c19b1f8`) | A native macOS build from before the repo reorg. It can't be reproduced. |
| Factory code, mainnet and testnet | hex `670ecb8001989c5dd36d8ff96f45bef138f3bac417134c67ac615768c57c4b40` | The old Docker CI build of `a26cd2f` (PR #166) |

The next release build will produce new hashes for both, even for unchanged code, because crate names, paths, the lockfile and the build image all changed in the reorg.

### Rust version

The toolchain is Rust 1.97.1, in `rust-toolchain.toml` and in the release image (`sourcescan/cargo-near:0.22.0-rust-1.97.1`) in each crate's `Cargo.toml`. Change them together, and pick a Rust version that has a [release image](https://hub.docker.com/r/sourcescan/cargo-near/tags).

- Rust 1.87 and later emit bulk-memory and non-trapping float-to-int wasm instructions. NEAR accepts them from protocol 84 (nearcore 2.12), and mainnet and testnet are past that.
- cargo-near only allows Rust above 1.86 when near-sdk declares protocol 84 or later as its minimum. near-sdk 5.28+ does, so **don't go back below near-sdk 5.28** without also going back to Rust 1.86.
- The test sandbox is neard 2.13.4, via `near-workspaces` 0.23.

## Which release do I need?

| What changed | Procedure | Affects |
|---|---|---|
| Trading account code (`contracts/trading-account`) | [Release trading account code](trading-account.md#release-new-trading-account-code) | Trading accounts created afterwards. The factory code isn't touched. |
| Factory code (`contracts/factory`) | [Release factory code](#release-factory-code) below | The factory only. It keeps its owner, MPC signer and trading account code hash. |
| A new factory account is needed | [Deploy a new factory](#deploy-a-new-factory) below | A new factory. Accounts created by the old one stay where they are. |

Existing trading accounts are never changed by a release. They keep the code they were created with.

## Release factory code

Use this for changes like bug fixes, gas tweaks and new view methods. A code-only redeploy keeps the factory's stored state. It only works if the new code reads the same stored fields, so if the `TradingAccountFactory` struct changed, deploy a new factory instead.

Who's involved:
- **Testnet:** whoever holds the `auth.peerfolio.testnet` key, which is in the keychain.
- **Mainnet:** the factory owner is the DAO `peerfolio.sputnik-dao.near`, and `auth.peerfolio.near` has no access keys. A council member adds a temporary key through a DAO proposal, deploys with it, and then deletes it. Council members sign with their Ledger, and proposals need 3 of 4 approvals.

### Step 1: Get the release build

1. Find the CI run for the commit you're releasing. Mainnet releases use a commit on `main`, from a run whose event is `push` or `workflow_dispatch`; check that it succeeded.
   ```bash
   export COMMIT=<release commit>
   gh run list --workflow contracts.yml --commit $COMMIT --json databaseId,event,conclusion
   ```
   If there's none, start one: `gh workflow run contracts.yml --ref main`. For a testnet rehearsal of unmerged code, start one on the branch instead (`--ref <branch>`).
2. Download the wasms in place of any local builds:
   ```bash
   rm -rf contracts/target/near
   gh run download <run id> -n contract-wasm -D contracts/target/near
   ```
3. Check the factory hash matches the one CI printed:
   ```bash
   shasum -a 256 contracts/target/near/trading_account_factory/trading_account_factory.wasm
   gh run view <run id> --log | grep "SHA-256 checksum"
   ```

Record the commit and the factory's hex hash. To check it independently, run `make release` on a clean checkout of the commit; it should print the same hash.

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
3. Create a trading account to check the new factory code works (see [trading-account.md](trading-account.md#lifecycle), step 1).

### Step 3: Add a temporary key to the mainnet factory

1. Generate the key outside the repo:
   ```bash
   export KEY_DIR=$(mktemp -d)
   near account create-account fund-later use-auto-generation save-to-folder $KEY_DIR
   export KEY_FILE=$(ls $KEY_DIR/*.json)
   export PUBKEY=$(jq -r .public_key $KEY_FILE)
   ```
2. Build the proposal. Set your own council account and Ledger path.
   ```bash
   export COUNCIL=<your council account>
   export HD_PATH="m/44'/397'/0'/0'/1'"
   export ARGS=$(echo -n "{\"public_key\":\"$PUBKEY\"}" | base64 | tr -d '\n')
   export KIND='{"FunctionCall":{"receiver_id":"auth.peerfolio.near","actions":[{"method_name":"add_full_access_key","args":"'$ARGS'","deposit":"0","gas":"15000000000000"}]}}'
   ```
3. Submit the proposal. The 0.1 NEAR is the proposal bond. The call returns the proposal ID.
   ```bash
   near contract call-function as-transaction peerfolio.sputnik-dao.near add_proposal json-args "{\"proposal\":{\"description\":\"Temporary key to deploy factory <commit>, hash <hex>\",\"kind\":$KIND}}" prepaid-gas '30.0 Tgas' attached-deposit '0.1 NEAR' sign-as $COUNCIL network-config mainnet sign-with-ledger --seed-phrase-hd-path "$HD_PATH" send
   ```
4. Check the proposal:
   ```bash
   near contract call-function as-read-only peerfolio.sputnik-dao.near get_proposal json-args '{"id":<ID>}' network-config mainnet now
   ```
5. Each council member votes. `KIND` must be exactly what was submitted, so share the `PUBKEY` value with the other voters; they set `ARGS` and `KIND` from it.
   ```bash
   near contract call-function as-transaction peerfolio.sputnik-dao.near act_proposal json-args "{\"id\":<ID>,\"action\":\"VoteApprove\",\"proposal\":$KIND}" prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as $COUNCIL network-config mainnet sign-with-ledger --seed-phrase-hd-path "$HD_PATH" send
   ```
6. After the third approval, check the key is on the factory:
   ```bash
   near account list-keys auth.peerfolio.near network-config mainnet now
   ```

### Step 4: Deploy on mainnet

```bash
near contract deploy auth.peerfolio.near use-file contracts/target/near/trading_account_factory/trading_account_factory.wasm without-init-call network-config mainnet sign-with-access-key-file $KEY_FILE send
```

Then run the same checks as step 2.2 with `auth.peerfolio.near` and `network-config mainnet`.

### Step 5: Delete the temporary key

Don't skip this. The factory should have no access keys.

```bash
near account delete-keys auth.peerfolio.near public-keys $PUBKEY network-config mainnet sign-with-access-key-file $KEY_FILE send
rm -rf $KEY_DIR
near account list-keys auth.peerfolio.near network-config mainnet now
```

## Deploy a new factory

Use this only for a new factory account, for example when the factory's stored state changes. [`contracts/factory/deploy.sh`](../contracts/factory/deploy.sh) does it in one go:

1. It creates `auth.peerfolio.<near|testnet>` from `peerfolio.<near|testnet>` with 4 NEAR.
2. It deploys the release factory wasm from `contracts/target/near` and calls `new` with the owner, the network and the trading account code hash.
3. It checks the deployed code hash matches the local wasm.

Get the release build first ([Release factory code, step 1](#step-1-get-the-release-build)), then:

```bash
contracts/factory/deploy.sh peerfolio.sputnik-dao.near 6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f mainnet
```

That's how the live mainnet factory was deployed. Run it with `peerfolio.peerfolio.testnet` and `testnet` first.

- The factory ID is hardcoded to `auth.peerfolio.<suffix>`. To use another ID, edit the script, and keep the ID at most 30 characters.
- `peerfolio.<near|testnet>`'s key has to be in your keychain. If that account uses a Ledger, temporarily add a keychain full-access key, run the script, then delete the key.
- The script uses near-cli-rs's legacy-compatible commands (`near deploy`, `near state`). In that syntax, `--deposit 1` means 1 NEAR, not 1 yoctoNEAR.
- If the factory account already exists, the script only redeploys the code and ignores the owner and hash arguments. That doesn't work on mainnet, where the factory has no keys; use [Release factory code](#release-factory-code) instead.
- The previous factory, `auth-v1.peerfolio.near`, was replaced in January 2026.

Afterwards, point the app at the new factory: `AUTH_CREATOR` / `VITE_AUTH_CREATOR_*` in ft-core (local `.env`, Render and Cloudflare).
