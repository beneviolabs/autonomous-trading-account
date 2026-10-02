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

Gotchas:
- **The first integration test run downloads the sandbox.** `near-workspaces` fetches neard 2.13.4 (the version mainnet runs) the first time the tests start a sandbox, and caches it. To run offline, or with a binary you already have, set `NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **Old sandboxes reject current wasm.** Wasm from Rust 1.87+ uses instructions that neard before 2.12 rejects with `CompilationError(PrepareError(Deserialization))`. If you see that error, check that `NEAR_SANDBOX_BIN_PATH` doesn't point at an older binary.
- **`test_transaction_bytes_unchanged` pins the signed transaction bytes.** If it fails after a dependency bump, the bytes the MPC signs, or the JSON hand-off between `request_signature` and `sign_request_callback`, have changed. Don't update the expected values without finding out why.
- **Rebuild the wasm before rerunning integration tests.** They embed the wasm at compile time, so after changing the contract, run `contracts/trading-account/build.sh` first, or use `scripts/test.sh`, which does.

## CI

`.github/workflows/contracts.yml` runs on a plain GitHub runner, with no custom image:
1. Installs the toolchain from `rust-toolchain.toml`, cargo-near 0.22.0 and cargo-audit.
2. Runs `make fmt-check`, `make clippy` (trading account only, since the factory currently fails clippy on a redundant `use bs58;`), `make test-unit` and `make audit`.
3. Runs `make release` (reproducible builds in NEAR's image), checks the trading account wasm is under 4 MiB, and uploads both wasms.

- Run any step locally with `make <target>`; `make help` lists them.
- The integration tests don't run in CI.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0285` (`rustls`) only reaches test dependencies and can't be fixed until near-sdk moves to near-crypto 0.38.

## End-to-end on a live network

<!-- TODO: document a repeatable testnet end-to-end check (create, authorize, request_signature, broadcast). -->

TODO.
