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
- **The first build can hang.** The `near-workspaces` build script downloads a `near-sandbox` binary at compile time and can stall for a long time. Point it at an existing binary to skip the download:
  ```bash
  export NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox
  ```
  Earlier builds leave a copy under `target/*/build/near-sandbox-utils-*/out/.near/near-sandbox-*/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **The integration tests need wasm built with Rust ≤ 1.86.** Otherwise they fail with `CompilationError(PrepareError(Deserialization))`. `build.sh` uses the version in `rust-toolchain.toml`.

## CI

`.github/workflows/contracts.yml` runs on a plain GitHub runner, with no custom image:
1. Installs the toolchain from `rust-toolchain.toml`, cargo-near 0.16.0 and cargo-audit.
2. Runs `make fmt-check`, `make clippy` (trading account only, since the factory currently fails clippy on a redundant `use bs58;`), `make test-unit` and `make audit`.
3. Runs `make release` (reproducible builds in NEAR's image), checks the trading account wasm is under 4 MiB, and uploads both wasms.

- Run any step locally with `make <target>`; `make help` lists them.
- The integration tests don't run in CI.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0009` (`time`) needs Rust 1.88 to fix and only reaches test dependencies. The dependency-review step in the workflow mirrors it.

## End-to-end on a live network

<!-- TODO: document a repeatable testnet end-to-end check (create, authorize, request_signature, broadcast). -->

TODO.
