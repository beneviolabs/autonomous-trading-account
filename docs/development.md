# Development

How to build and test the contracts, from a change on your machine to a release. Releasing itself is in [Releasing](releasing.md).

## Prerequisites

- rustup. `rust-toolchain.toml` pins Rust 1.97.1, and rustup installs it automatically.
- cargo-near 0.22.0: `cargo install cargo-near --version 0.22.0 --locked`. The build scripts refuse other versions.
- [near-cli-rs](https://github.com/near/near-cli-rs), tested with 0.22. Run `near login` for each account you'll sign as.
- Docker, only to reproduce a release build locally. Releases normally use CI's build.
- For the release and dev scripts: `gh` (signed in), `jq`, `curl` and `python3`. `shellcheck` to lint them.

## From change to release

0. **Follow the [rules for changing the trading account](#changing-the-trading-account)**: which version to bump, and how to migrate state.
1. **Unit tests** on a mocked chain: `make test-unit`.
2. **Sandbox tests** against a local neard with a stub `intents.near` and a live 1Click quote: `make test-integration`.
3. **Try it on one live account**, when the change touches `intents.near` or 1Click: a [dev session](dev-session.md) flashes your checkout onto a mainnet trading account and puts it back afterwards.
4. **CI** runs all the checks on the pull request ([CI](#ci)).
5. **Release** it once merged, with the [runbook](releasing.md#which-release-do-i-need) for what changed. Each trading account release ends with the [smoke test](runbooks/smoke-test.md) on mainnet.

## Changing the trading account

Rules for any change to `contracts/trading-account`. Accounts upgrade only when their owner signs, so the fleet always runs several versions at once, and every release has to work for accounts on any of them.

### Which version to bump

Both constants are at the top of `lib.rs`. `CONTRACT_VERSION` is semver over the interface: the public methods, their arguments and results, and who may call them. `STATE_VERSION` counts the shapes of the stored `STATE`.

| Change | `CONTRACT_VERSION` | `STATE_VERSION` |
|---|---|---|
| A bug fix or internal change with no interface change; gas constants; logging | patch | — |
| A new public method, a new optional argument, or a new field in a view result | minor | — |
| Letting agents do more, or loosening an access check | minor | — |
| Removing or renaming a public method, or changing its required arguments or result | major | — |
| Letting agents do less, or tightening an access check | major | — |
| Renaming a `#[private]` callback, or changing its arguments | major | — |
| Any change to the stored bytes of `STATE` ([below](#what-needs-a-new-state-version)) | major | bump |

- **Private callbacks are interface too.** A callback scheduled before an upgrade runs on the code that's live when it fires. If an upgrade lands between `swap` and `swap_callback`, the new code must still have `swap_callback` with the same arguments, or that swap fails. A release that renames a callback keeps the old name as well, and removes it a release later.
- **A state change is always a major**, even if no method changes, because it's one-way: older code panics on the new bytes ([rollbacks](#rollbacks)).

### What needs a new state version

What matters is the stored bytes, not the Rust types. A change that leaves the bytes identical needs no new version, and one that changes them needs one even if it looks cosmetic. `test_state_bytes_match_the_snapshot` compares a fixed fixture against `schema/state_v{N}.hex`, and catches most of these:

| Change | New state version? | If it ships without one, it fails… |
|---|---|---|
| A wrapper type with identical bytes | No | — |
| Appending a field | Yes | Loudly: old bytes are too short and the parse panics |
| Removing a field | Yes | Loudly: trailing bytes and the parse panics |
| **Reordering fields** (including enum variants) | Yes | **Silently** when lengths match: swapped fields of the same type parse cleanly. Append instead, and flag any reorder in review |
| **Changing a collection prefix** (`b"a"`) | Yes | **Silently**: every `ai`/`ae` key is orphaned and the agent set reads empty, with no panic |
| Changing a collection type (`collections::UnorderedSet` → `store::IterableSet`) | Yes | The migration must move the elements and delete the old keys in the same call |

### Writing a migration

`migrate` runs in the same receipt as the code swap, so a panic in it reverts the swap. It matches on the stored version byte, `STATE[0]`, and brings the state up to `STATE_VERSION` in one call. When you bump `STATE_VERSION` to N:

1. Keep the old shape as a `StateV{N-1}` struct, used only to read old bytes, and add its arm to `migrate`.
2. Snapshot the new shape as `schema/state_v{N}.hex`, and keep the old snapshot.
3. Add the old version's row to the [release gate](#release-gate).

Rules every migration follows:
- **Every wasm carries every migration.** An account any number of versions behind reaches the current one in a single `upgrade`, without ever running the code in between.
- **Idempotent.** At `STATE_VERSION`, `migrate` leaves the state unchanged, so retries and code-only releases are harmless.
- **No promises or cross-contract calls.** A promise is a new receipt, which the revert no longer covers.
- **Move collections, never rebuild them.** Carry a set into the new struct as it is, so its prefix and the entries stored under it don't change. A fresh `UnorderedSet::new(b"a")` reads empty.
- **Keep state bounded.** A few fields and at most `MAX_AGENTS` (10) agents is what keeps every migration affordable in one receipt. An unbounded map would force lazy migration and break this scheme.
- **Log everything a bad migration could lose.** The `migrated` event (NEP-297) holds the raw `STATE` bytes and every collection's elements, read before the change. Collections live under their own keys, so `STATE` alone wouldn't be enough. A new collection goes in the event in the release that adds it. Only public data belongs in state, so the event can carry all of it.
- **The v0 arm works only while `STATE_VERSION` is below 64.** v0 state has no version byte, and its first byte is always 64, the owner's length. A `const` assertion refuses to build otherwise. Remove the v0 arm, and the assertion, once no account runs v0.
- **Remove a migration only once no account runs a hash at or below its source version,** checked on chain, never from the app's database. The app's minimum supported version moves first, and the removal comes a release later.

### Rollbacks

- **Moving the pointer back** to the last good release stops new accounts, and accounts that haven't upgraded yet, from getting the bad one. That's only safe if the last good release has the same `STATE_VERSION`. Otherwise fix forward.
- **Accounts that already upgraded only move forward.** Moving the pointer doesn't change their code. Ship a fixed release with a higher `CONTRACT_VERSION`. If the stored state needs repair, bump `STATE_VERSION` and repair it in the migration, using what the `migrated` event logged.
- **A broken migration is fixed forward too.** Edit its `StateV{N}` arm in the next release. Accounts that haven't upgraded yet get the fix. A released state *shape* never changes: a fix that needs a different shape is a new state version.
- **Downgrades fail by design.** Older code panics on newer state, and the receipt reverts. But a published global contract can't be unpublished, so any full-access key can still deploy an old hash onto an account. Only the account's own signature stops it.

### Release gate

The sandbox tests prove that every state version still carried survives this release's `migrate`. For each version (a "row"), they deploy that version's released wasm, add an owner and 10 agents, upgrade to the current build, and check the owner and all 10 agents come back. Checking all 10 is what catches a rebuilt collection reading empty. v0's row uses `res/v0.wasm`, fetched from mainnet. Alongside them:
- `test_state_bytes_match_the_snapshot` fails when the state changes without a version bump.
- `test_upgrade_reverts_when_migrate_panics` and `test_v0_bootstrap_reverts_when_migrate_panics` force `migrate` to panic, and check that the code swap reverts and the failure surfaces in the top-level status.

A release that bumps `STATE_VERSION` commits the previous version's released wasm, downloaded from its GitHub Release, as a new row. Removing a migration removes its row.

## Tests

```bash
make test               # all of the below
make test-unit          # unit tests, one crate at a time
make test-integration   # rebuilds the trading account, then runs the sandbox tests
make shellcheck         # lints scripts/
```

The integration tests load `contracts/target/near/trading_account/trading_account.wasm` (this checkout's build), or the file `TRADING_ACCOUNT_WASM` names, and only compile with `--features integration-tests`. Run them through `make test-integration`, which rebuilds the wasm first; `cargo test --features integration-tests` on its own runs them against whatever wasm was built last. A bare `cargo test` doesn't run them at all. To pass test filters or `--nocapture`, run the `cargo test` line from the `Makefile` after it.

Test one package per command (`make test-unit` does). The contracts enable different near-sdk features, and `--workspace` would unify them.

Writing tests:
- Name them `test_<method>_<behaviour>`, for example `test_create_proxy_global_rejects_non_owner_predecessor`.
- For a method that returns a `Promise`, drop the promise and check what it scheduled with `near_sdk::test_utils::get_created_receipts()`. Checking that a `Promise` came back proves nothing.

Where `swap` is tested:
- `unit_tests.rs`: the quote checks (`test_verify_*`, `test_check_*`, `test_parse_deadline_ms_*`) and `swap` on the mocked chain (`test_swap_*`).
- `integration_tests.rs`: `swap`'s cross-contract call in the sandbox (`test_swap_*`), with a live 1Click quote against a stub `intents.near`.
- Only a mainnet run checks the real `intents.near` and a whole swap: a [dev session](dev-session.md) before merging, and the [smoke test](runbooks/smoke-test.md) after a release.

**The sandbox tests need the network.** They fetch one live 1Click quote per run, from 1Click's quote API with `curl`, paying out to a mainnet trading account the sandbox patches in. Nothing is deposited, so no funds move. Because the contract trusts only 1Click's key, the sandbox tests run the same wasm that's released, and `scripts/release/deploy-global.sh` runs them against a release before deploying it. An expired quote is only tested in the unit tests, since 1Click won't sign one.

Gotchas:
- **The first integration test run downloads the sandbox.** `near-workspaces` fetches neard 2.13.4 (the version mainnet runs) the first time the tests start a sandbox, and caches it. To run offline, or with a binary you already have, set `NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **Old sandboxes reject current wasm.** Wasm from Rust 1.87+ uses instructions that neard before 2.12 rejects with `CompilationError(PrepareError(Deserialization))`. If you see that error, check that `NEAR_SANDBOX_BIN_PATH` doesn't point at an older binary.

## Builds

| | How | Deploy it? |
|---|---|---|
| Release | Trading account: its [GitHub Release](releasing.md#versions). Factory: the `contract-wasm` artifact from CI. Or `make release` locally. | Yes. Only deploy these. The release scripts download and check them. |
| Dev | `make build` | No, except onto one account in a [dev session](dev-session.md). |

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

### Sharing code between the contracts

The contracts form one Cargo workspace in `contracts/`, with a shared `Cargo.lock` and `target/`. To share code between them, add a library crate to the workspace that both depend on by path. Keep it to plain types; it must not define a `#[near]` contract. A good first candidate is the factory's `TradingAccountInitArgs`, which has to match the arguments the trading account's `new` reads. Today only a factory unit test checks the arguments the factory sends.

## CI

`.github/workflows/contracts.yml` runs on plain GitHub runners, with no custom image, for pull requests, pushes to `main` and [release tags](releasing.md#versions). It has three jobs:
- **`test`**: installs the toolchain from `rust-toolchain.toml`, cargo-near 0.22.0 and cargo-audit, then runs `make fmt-check`, `make clippy`, `make shellcheck`, `make test` (unit and sandbox integration tests) and `make audit`. The neard sandbox binary is cached by version and passed in with `NEAR_SANDBOX_BIN_PATH`; when near-workspaces changes its default sandbox version, update `NEAR_SANDBOX_VERSION` in the workflow.
- **`build`**, in parallel with `test`: runs `make release` (reproducible builds in NEAR's image), and checks both wasms are under 4 MiB. For pushes to `main`, release tags and manual runs, it uploads both wasms for [releases](#builds).
- **`release`**, for release tag pushes only (not manual runs), after `test` and `build` pass: checks the tag is on `main`, matches `CONTRACT_VERSION` and is the newest, then publishes the trading account wasm as a GitHub Release ([Versions](releasing.md#versions)).

- Run any step locally with `make <target>`; `make help` lists them.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0285` (`rustls`) only reaches test dependencies and can't be fixed until near-sdk moves to near-crypto 0.38. The dependency-review step in the workflow mirrors it.
