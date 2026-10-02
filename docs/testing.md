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

What covers `request_signature`, the path that builds and signs transactions:
- **`test_transaction_bytes_unchanged`** pins the bytes the MPC signs and the final signed transaction to what the audited version produced. If it fails after a dependency bump, the encoding or the JSON hand-off between `request_signature` and `sign_request_callback` changed. Don't update the expected values without finding out why.
- **`sign_request_callback` unit tests** feed it a response in the real signer's format, signed with a fixed secp256k1 key by `test_support.rs` (success and signer failure).
- **`test_request_signature_with_stub_signer`** runs the whole call in the sandbox with the documented minimum of 100 Tgas, using a tiny WAT contract in place of `v1.signer`.
- Only a testnet run checks the real signer.

Gotchas:
- **The first integration test run downloads the sandbox.** `near-workspaces` fetches neard 2.13.4 (the version mainnet runs) the first time the tests start a sandbox, and caches it. To run offline, or with a binary you already have, set `NEAR_SANDBOX_BIN_PATH=/path/to/near-sandbox`.
- **Debug assertions are off in the test profile.** `[profile.test] debug-assertions = false` is in the workspace `contracts/Cargo.toml` because near-sdk's mocked blockchain trips Rust's debug-only pointer precondition check (`unsafe precondition(s) violated: ptr::replace …`) and aborts the test binary. Don't remove it.
- **Old sandboxes reject current wasm.** Wasm from Rust 1.87+ uses instructions that neard before 2.12 rejects with `CompilationError(PrepareError(Deserialization))`. If you see that error, check that `NEAR_SANDBOX_BIN_PATH` doesn't point at an older binary.
- **Rebuild the wasm before rerunning integration tests.** They embed the wasm at compile time, so after changing the contract, run `contracts/trading-account/build.sh` first, or use `scripts/test.sh`, which does.

## CI

`.github/workflows/contracts.yml` runs on a plain GitHub runner, with no custom image:
1. Installs the toolchain from `rust-toolchain.toml`, cargo-near 0.22.0 and cargo-audit.
2. Runs `make fmt-check`, `make clippy`, `make test` (unit and sandbox integration tests) and `make audit`.
3. Runs `make release` (reproducible builds in NEAR's image), and checks both wasms are under 4 MiB. For pushes to `main` and manual runs, it uploads both wasms for [releases](releases.md#builds).

- Run any step locally with `make <target>`; `make help` lists them.
- `cargo audit` ignores are in `contracts/.cargo/audit.toml`, each with a reason. `RUSTSEC-2026-0285` (`rustls`) only reaches test dependencies and can't be fixed until near-sdk moves to near-crypto 0.38. The dependency-review step in the workflow mirrors it.

## End-to-end on a live network

The sandbox tests use a stub signer, so only a testnet run checks the real MPC signer's response. Do this for any change to how `request_signature` builds, signs or returns transactions, before release.

It runs unreleased trading account code on `auth-v2.peerfolio.testnet`, a retired testnet factory kept for this. Its key is in the keychain, and the account is its own owner and authorized user. It needs about 3.2 NEAR locked for the contract's storage; keep at least 4 NEAR on it.

1. Build the code under test: `contracts/trading-account/build.sh`.
2. Deploy it. Redeploying over the trading account keeps its state, so skip `new` if it's already initialized:
   ```bash
   near contract deploy auth-v2.peerfolio.testnet use-file contracts/target/near/trading_account/trading_account.wasm without-init-call network-config testnet sign-with-keychain send
   ```
   If the code under test changes the stored state, or the account holds another contract's state, clear it first: deploy `state_cleanup.wasm` from [near-clear-state](https://github.com/doriancrutcher/near-clear-state), call `clean` with `{"keys":["U1RBVEU="]}` (base64 for `STATE`), then deploy with `with-init-call new json-args '{"owner_id":"auth-v2.peerfolio.testnet","signer_id":"v1.signer-prod.testnet"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR'`.
3. Run [lifecycle](trading-account.md#lifecycle) steps 2, 3 and 5 with `$TA`, `$OWNER` and `$AGENT` all set to `auth-v2.peerfolio.testnet`, signing every command with `sign-with-keychain`. Step 4 isn't needed; the account's own balance pays.
4. It passes if `request_signature` returns a signed transaction, the broadcast succeeds, and `ft_balance_of` on `wrap.testnet` shows the wNEAR.
5. Clean up, so the next run starts the same way: unwrap with `near_withdraw` (`{"amount":"<wNEAR balance>"}`, 1 yoctoNEAR) and call `storage_unregister` (1 yoctoNEAR) on `wrap.testnet`; `remove_authorized_user` for `auth-v2.peerfolio.testnet`; and delete the MPC key with `near account delete-keys auth-v2.peerfolio.testnet public-keys <MPC key> network-config testnet sign-with-keychain send`.

Last run: 2026-10-02, for #168 ([request_signature](https://testnet.nearblocks.io/txns/BHvjRyc8YG3NJHi8fsPCk44zvMxtoxvtAA6JdzkU2576), [broadcast](https://testnet.nearblocks.io/txns/8EJxJ2B3XajLmUhyLttihAUKCxkwFNwpLxm9H1T1Cs97)).
