# Testing

## Local

```bash
make test               # all of the below
make test-unit          # unit tests, one crate at a time
make test-integration   # rebuilds the trading account, then runs its sandbox tests
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
- Only a mainnet run checks the real `intents.near` and 1Click: the [smoke test](runbooks/smoke-test.md).

**The sandbox build trusts a test key.** `make test-integration` builds the wasm with `ONE_CLICK_TEST_KEY` set, so the contract also accepts quotes signed with the tests' key and the sandbox tests can sign their own. Don't set it for any other build. `scripts/flash-dev.sh` builds without it and refuses a wasm that contains the key. Release builds pass no environment into their container, so they trust 1Click's key alone.

Gotchas:
- **The first integration test run downloads the sandbox.** `near-workspaces` fetches neard 2.13.4 (the version mainnet runs) the first time the tests start a sandbox, and caches it. To run offline, or with a binary you already have, set `NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **Old sandboxes reject current wasm.** Wasm from Rust 1.87+ uses instructions that neard before 2.12 rejects with `CompilationError(PrepareError(Deserialization))`. If you see that error, check that `NEAR_SANDBOX_BIN_PATH` doesn't point at an older binary.

## CI

`.github/workflows/contracts.yml` runs on plain GitHub runners, with no custom image, for pull requests, pushes to `main` and [release tags](releases.md#versions). It has three jobs:
- **`test`**: installs the toolchain from `rust-toolchain.toml`, cargo-near 0.22.0 and cargo-audit, then runs `make fmt-check`, `make clippy`, `make shellcheck`, `make test` (unit and sandbox integration tests) and `make audit`. The neard sandbox binary is cached by version and passed in with `NEAR_SANDBOX_BIN_PATH`; when near-workspaces changes its default sandbox version, update `NEAR_SANDBOX_VERSION` in the workflow.
- **`build`**, in parallel with `test`: runs `make release` (reproducible builds in NEAR's image), and checks both wasms are under 4 MiB. For pushes to `main`, release tags and manual runs, it uploads both wasms for [releases](releases.md#builds).
- **`release`**, for release tag pushes only (not manual runs), after `test` and `build` pass: checks the tag is on `main`, matches `CONTRACT_VERSION` and is the newest, then publishes the trading account wasm as a GitHub Release ([Versions](releases.md#versions)).

- Run any step locally with `make <target>`; `make help` lists them.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0285` (`rustls`) only reaches test dependencies and can't be fixed until near-sdk moves to near-crypto 0.38. The dependency-review step in the workflow mirrors it.

## End-to-end on a live network

Each trading account release is checked on mainnet with the [smoke test](runbooks/smoke-test.md), on an account from the dev factory.
