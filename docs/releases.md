# Builds and releases

How both contracts are built, and how new code reaches testnet and mainnet. Accounts and owners are in the [README](../README.md#deployments).

## Builds

| | How | Deploy it? |
|---|---|---|
| Release | Trading account: its [GitHub Release](#versions). Factory: the `contract-wasm` artifact from CI. Or `make release` locally. | Yes. Only deploy these. The release scripts download and check them. |
| Dev | `make build` | No, except onto one account for a [dev session](runbooks/dev-deploy.md). |

Both write to `contracts/target/near/<crate>/<crate>.wasm`:
- trading account: `contracts/target/near/trading_account/trading_account.wasm`
- factory: `contracts/target/near/trading_account_factory/trading_account_factory.wasm`

- **Release builds are reproducible.** They run `cargo near build reproducible-wasm` in NEAR's build image, pinned by digest under `[package.metadata.near.reproducible_build]` in each crate's `Cargo.toml`. The same commit gives the same hash on any machine. The wasm also records the repository, commit and build image (NEP-330), so explorers can verify it against the source.
- **Release wasm comes from CI.** The `contracts` workflow runs `make release`, prints each wasm's SHA-256 (hex and bs58) in its log, and uploads both wasms as the `contract-wasm` artifact, kept for 90 days. The release scripts take the factory from there, and the trading account from its GitHub Release.
  - CI uploads the artifact only for pushes to `main`, release tags and manual runs. Pull request and other branch runs build and test but don't upload.
  - Mainnet releases come from `main`. A manual run on a branch (`gh workflow run contracts.yml --ref <branch>`) is for testnet and dev only.
  - CI runs on pull requests and on pushes to `main` that touch contract files. Otherwise, start a run with `gh workflow run contracts.yml --ref main`.
- **`make release` locally** needs Docker and a clean tree with everything committed, including `Cargo.lock`. NEAR's build images are x86-only, so on Apple Silicon Docker emulates them and the build is slow. Use it to check a CI hash independently, not as the usual route.
- **To change the build image**, update `image` and `image_digest` in both crates together. That changes both hashes.
- **Dev builds aren't reproducible.** They compile natively, and the same commit produces different code on macOS arm64 and on Linux x86_64.

### Rust version

The toolchain is Rust 1.97.1, in `rust-toolchain.toml` and in the release image (`sourcescan/cargo-near:0.22.0-rust-1.97.1`) in each crate's `Cargo.toml`. Change them together, and pick a Rust version that has a [release image](https://hub.docker.com/r/sourcescan/cargo-near/tags).

- Rust 1.87 and later emit bulk-memory and non-trapping float-to-int wasm instructions. NEAR accepts them from protocol 84 (nearcore 2.12), and mainnet and testnet are past that.
- cargo-near only allows Rust above 1.86 when near-sdk declares protocol 84 or later as its minimum. near-sdk 5.28+ does, so **don't go back below near-sdk 5.28** without also going back to Rust 1.86.
- The test sandbox is neard 2.13.4, via `near-workspaces` 0.23.

### v0

The trading account code that accounts created before 1.0.0 run, global hash `6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f`, has no GitHub Release. It was a native macOS build from before the repo reorg and can't be reproduced, so the release-gate tests use a copy fetched from mainnet, `contracts/trading-account/res/v0.wasm`, and the scripts know its hash as `v0`.

To see what each factory runs and points at now, run `scripts/status.sh` (or `scripts/status.sh testnet`).

## Versions

The trading account reports its version from `contract_version()`. The numbers are two constants at the top of `contracts/trading-account/src/lib.rs`:
- `CONTRACT_VERSION`: bumped once per release. The Peerfolio Backend trading-account upgrades RFC says whether it's a patch, minor or major bump.
- `STATE_VERSION`: bumped only in the PR that changes the shape of the stored state. A unit test fails if the state changes without it.

**A release is a git tag**, `trading-account/v<CONTRACT_VERSION>`. Pushing the tag is what releases. CI then:
1. runs the tests and the reproducible build of the tagged commit,
2. checks the tag is on `main`, matches `CONTRACT_VERSION` in that commit and is the newest release tag,
3. publishes a GitHub Release holding `trading_account.wasm` and its hashes.

GitHub Releases are kept forever, unlike CI artifacts (90 days), so each release's wasm and hashes stay one click from its version. A version can't be released twice: a second Release for the same tag fails. Skipped version numbers are fine.

### 1.0.0

The first versioned release, and the first `upgrade` and `migrate`. Agents swap with `swap(quote_payload, quote_signature)`, which deposits into a 1Click quote with a direct `mt_transfer` call. The owner and agents can move up to 10 tokens from the trading account's intents balance to the owner's with `transfer_to_owner(tokens, memo)`. `request_signature`, the MPC signing path, `get_signer_id` and `set_signer_id` are gone, and migrating from v0 drops `signer_id` from the state.

## Which release do I need?

| What changed | Procedure | Affects |
|---|---|---|
| Trading account code (`contracts/trading-account`) | [Release trading account code](#release-trading-account-code) | Trading accounts created afterwards, and existing ones once their owner [upgrades](trading-account.md#upgrading-a-trading-account). The factory code isn't touched. |
| Factory code (`contracts/factory`) | [Release factory code](#release-factory-code) | The factory only. It keeps its owner, MPC signer and trading account code hash. |
| A new factory account is needed | [Deploy a new factory](factory.md#deploy-a-new-factory) | A new factory. Accounts created by the old one stay where they are. |

A release never changes an existing trading account by itself. Each keeps its code until its owner [upgrades it](trading-account.md#upgrading-a-trading-account) to the hash the factory points at.

Who's involved on mainnet: the factory owner is the DAO `peerfolio.sputnik-dao.near`, so every change to `auth.peerfolio.near` is a [DAO proposal](#dao-proposals). On testnet, the factory owner `peerfolio.peerfolio.testnet` and the factory account `auth.peerfolio.testnet` sign directly from the keychain. The mainnet [dev factory](factory.md#dev-factory) `auth.dev.peerfolio.near` and its owner `dev.peerfolio.near` sign from the keychain too.

## DAO proposals

Each mainnet change to the prod factory is one function call on `auth.peerfolio.near`, proposed to the DAO with `scripts/release/dao-propose.sh`. It needs 3 of 4 council approvals, and council members vote however they like, in Trezu or the near CLI.

## Release trading account code

New trading account code goes out as a new global contract, and the factories are then pointed at its hash: dev first, prod through the DAO once the dev rehearsal passes and the app that handles the new version is on prod. The factory code isn't redeployed. Follow the [runbook](runbooks/release-trading-account.md).

## Release factory code

A code-only redeploy keeps the factory's stored state, so it only works if the new code reads the same stored fields. Follow the [runbook](runbooks/release-factory.md).

`auth.peerfolio.near` has no access keys, so on mainnet the DAO adds the deploying council member's key, they deploy with it, and the script deletes it.

> **TODO: deploy factory code without a temporary key.** For the length of a release, one council member alone holds a full-access key to the factory, outside the DAO's 3-of-4 approval. Instead, the factory could have an owner-only method that deploys the wasm passed to it onto its own account. Sputnik DAO already supports this: `store_blob` uploads the wasm to the DAO, and an `UpgradeRemote` proposal calls that method with it once approved. The factory would never need an access key. That's a factory code change, so it needs review, and the first release that adds the method still has to go out this way.
