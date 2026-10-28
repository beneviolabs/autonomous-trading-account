# Development

How to build and test the contracts, from a change on your machine to a release. Releasing itself is in [Releasing](releasing.md).

## Prerequisites

- rustup. `rust-toolchain.toml` pins Rust 1.97.1, and rustup installs it automatically.
- cargo-near 0.22.0: `cargo install cargo-near --version 0.22.0 --locked`. The build scripts refuse other versions.
- [near-cli-rs](https://github.com/near/near-cli-rs), tested with 0.22. Run `near login` for each account you'll sign as.
- Docker, only to reproduce a release build locally. Releases normally use CI's build.
- For the release and dev scripts: `gh` (signed in), `jq`, `curl` and `python3`. `shellcheck` to lint them.

## From change to release

1. **Unit tests** on a mocked chain: `make test-unit`.
2. **Sandbox tests** against a local neard with a stub `intents.near`: `make test-integration`.
3. **Try it on one live account**, when the change touches `intents.near` or 1Click: a [dev session](dev-session.md) flashes your checkout onto a mainnet trading account and puts it back afterwards.
4. **CI** runs all the checks on the pull request ([CI](#ci)).
5. **Release** it once merged, with the [runbook](releasing.md#which-release-do-i-need) for what changed. Each trading account release ends with the [smoke test](runbooks/smoke-test.md) on mainnet.

## Tests

```bash
make test               # all of the below
make test-unit          # unit tests, one crate at a time
make test-integration   # rebuilds the trading account, then runs the sandbox tests
make shellcheck         # lints scripts/
```

The integration tests embed `contracts/target/near/trading_account/trading_account.wasm` (a dev build) at compile time and only compile with `--features integration-tests`. Run them through `make test-integration`, which rebuilds the wasm first; a bare `cargo test` runs them against whatever wasm was built last. To pass test filters or `--nocapture`, run the `cargo test` line from the `Makefile` after it.

Test one package per command (`make test-unit` does). The contracts enable different near-sdk features, and `--workspace` would unify them.

Writing tests:
- Name them `test_<method>_<behaviour>`, for example `test_create_proxy_global_rejects_non_owner_predecessor`.
- For a method that returns a `Promise`, drop the promise and check what it scheduled with `near_sdk::test_utils::get_created_receipts()`. Checking that a `Promise` came back proves nothing.

Where `swap` is tested:
- `unit_tests.rs`: the quote checks (`test_verify_*`, `test_check_*`, `test_parse_deadline_ms_*`) and `swap` on the mocked chain (`test_swap_*`).
- `integration_tests.rs`: `swap`'s cross-contract call in the sandbox (`test_swap_*`), against a stub `intents.near`.
- Only a mainnet run checks the real `intents.near` and 1Click: a [dev session](dev-session.md) before merging, and the [smoke test](runbooks/smoke-test.md) after a release.

**The sandbox build trusts a test key.** `make test-integration` builds the wasm with `ONE_CLICK_TEST_KEY` set, so the contract also accepts quotes signed with the tests' key and the sandbox tests can sign their own. Don't set it for any other build. `scripts/dev/flash-dev.sh` builds without it and refuses a wasm that contains the key. Release builds pass no environment into their container, so they trust 1Click's key alone.

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
