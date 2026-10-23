# Testing

## Local

```bash
scripts/test.sh
```

This runs the trading account and factory unit tests, then builds the trading account and runs its sandbox integration tests. The integration tests embed `contracts/target/near/trading_account/trading_account.wasm` (a dev build) at compile time and only compile with `--features integration-tests`. The script stops at the first failure.

Each crate on its own:

```bash
cd contracts
cargo test -p trading-account --lib
cargo test -p trading-account-factory --lib
cargo test -p trading-account --features integration-tests --lib integration_tests   # needs contracts/trading-account/build.sh first
```

Test one package per command (`make test-unit` does). The contracts enable different near-sdk features, and `--workspace` would unify them.

Writing tests:
- Name them `test_<method>_<behaviour>`, for example `test_create_proxy_global_rejects_non_owner_predecessor`.
- For a method that returns a `Promise`, drop the promise and check what it scheduled with `near_sdk::test_utils::get_created_receipts()`. Checking that a `Promise` came back proves nothing.

Where `swap` is tested:
- `unit_tests.rs`: the quote checks (`test_verify_*`, `test_check_*`, `test_parse_deadline_ms_*`) and `swap` on the mocked chain (`test_swap_*`).
- `integration_tests.rs`: `swap`'s cross-contract call in the sandbox (`test_swap_*`), against a stub `intents.near`.
- Only a mainnet run checks the real `intents.near` and 1Click.

**The sandbox build trusts a test key.** `scripts/test.sh` builds the wasm with `ONE_CLICK_TEST_KEY` set, so the contract also accepts quotes signed with the tests' key and the sandbox tests can sign their own. Don't set it for any other build, and never deploy a dev build. Release builds pass no environment into their container, so they trust 1Click's key alone.

Gotchas:
- **The first integration test run downloads the sandbox.** `near-workspaces` fetches neard 2.13.4 (the version mainnet runs) the first time the tests start a sandbox, and caches it. To run offline, or with a binary you already have, set `NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **Old sandboxes reject current wasm.** Wasm from Rust 1.87+ uses instructions that neard before 2.12 rejects with `CompilationError(PrepareError(Deserialization))`. If you see that error, check that `NEAR_SANDBOX_BIN_PATH` doesn't point at an older binary.
- **Rebuild the wasm before rerunning integration tests.** They embed the wasm at compile time, so after changing the contract, run `contracts/trading-account/build.sh` first, or use `scripts/test.sh`, which does.

## CI

`.github/workflows/contracts.yml` runs on plain GitHub runners, with no custom image, for pull requests, pushes to `main` and [release tags](releases.md#versions). It has three jobs:
- **`test`**: installs the toolchain from `rust-toolchain.toml`, cargo-near 0.22.0 and cargo-audit, then runs `make fmt-check`, `make clippy`, `make test` (unit and sandbox integration tests) and `make audit`. The neard sandbox binary is cached by version and passed in with `NEAR_SANDBOX_BIN_PATH`; when near-workspaces changes its default sandbox version, update `NEAR_SANDBOX_VERSION` in the workflow.
- **`build`**, in parallel with `test`: runs `make release` (reproducible builds in NEAR's image), and checks both wasms are under 4 MiB. For pushes to `main`, release tags and manual runs, it uploads both wasms for [releases](releases.md#builds).
- **`release`**, for release tag pushes only (not manual runs), after `test` and `build` pass: checks the tag is on `main`, matches `CONTRACT_VERSION` and is the newest, then publishes the trading account wasm as a GitHub Release ([Versions](releases.md#versions)).

- Run any step locally with `make <target>`; `make help` lists them.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0285` (`rustls`) only reaches test dependencies and can't be fixed until near-sdk moves to near-crypto 0.38. The dependency-review step in the workflow mirrors it.

## End-to-end on a live network

<!-- TODO: document a repeatable mainnet end-to-end check (create, authorize, swap, liquidate). -->

TODO.
