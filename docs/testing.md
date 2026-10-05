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

What covers `request_signature`, the path that builds and signs transactions:
- **`test_transaction_bytes_unchanged`** pins the bytes the MPC signs and the final signed transaction to what the audited version produced. If it fails after a dependency bump, the encoding or the JSON hand-off between `request_signature` and `sign_request_callback` changed. Don't update the expected values without finding out why.
- **`create_signature_request` unit tests** pin the JSON sent to the MPC signer's `sign`, with the sha256 payload computed outside the contract.
- **`sign_request_callback` unit tests** feed it a response in the real signer's format, signed with a fixed secp256k1 key by `test_support.rs` (success and signer failure). `test_sign_request_callback_keeps_every_deposit` checks that deposits reach the signed transaction unchanged, including ones whose digits prefix each other (1 and 10, the pen test #8 case), 0 and `u128::MAX`.
- **`test_request_signature_with_stub_signer`** runs the whole call in the sandbox with the documented minimum of 100 Tgas, using a tiny WAT contract in place of `v1.signer`. Its two `mt_transfer` calls carry deposits 1 and 10, so it also covers the deposit case through the real JSON hand-off.
- Only a testnet run checks the real signer.

Gotchas:
- **The first integration test run downloads the sandbox.** `near-workspaces` fetches neard 2.13.4 (the version mainnet runs) the first time the tests start a sandbox, and caches it. To run offline, or with a binary you already have, set `NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **Old sandboxes reject current wasm.** Wasm from Rust 1.87+ uses instructions that neard before 2.12 rejects with `CompilationError(PrepareError(Deserialization))`. If you see that error, check that `NEAR_SANDBOX_BIN_PATH` doesn't point at an older binary.
- **Rebuild the wasm before rerunning integration tests.** They embed the wasm at compile time, so after changing the contract, run `contracts/trading-account/build.sh` first, or use `scripts/test.sh`, which does.

## CI

`.github/workflows/contracts.yml` runs on plain GitHub runners, with no custom image, for pull requests and for pushes to `main`. It has two parallel jobs:
- **`test`**: installs the toolchain from `rust-toolchain.toml`, cargo-near 0.22.0 and cargo-audit, then runs `make fmt-check`, `make clippy`, `make test` (unit and sandbox integration tests) and `make audit`. The neard sandbox binary is cached by version and passed in with `NEAR_SANDBOX_BIN_PATH`; when near-workspaces changes its default sandbox version, update `NEAR_SANDBOX_VERSION` in the workflow.
- **`build`**: runs `make release` (reproducible builds in NEAR's image), and checks both wasms are under 4 MiB. For pushes to `main` and manual runs, it uploads both wasms for [releases](releases.md#builds).

- Run any step locally with `make <target>`; `make help` lists them.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0285` (`rustls`) only reaches test dependencies and can't be fixed until near-sdk moves to near-crypto 0.38. The dependency-review step in the workflow mirrors it.

## End-to-end on a live network

<!-- TODO: document a repeatable testnet end-to-end check (create, authorize, request_signature, broadcast). -->

TODO.
