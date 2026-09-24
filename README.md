#### MPC Secured Agent Autonomy

1. [The limited access autonomous trading account](https://github.com/beneviolabs/ft-allowance-agent/blob/main/contracts/auth_proxy.rs) manages authorized users for signature requests, allows one to transfer tokens to their trading account and grant an AI agent (i.e any other Near Account) permission to call this proxy contract to request MPC approval to send transactions to a predefined set of contracts and methods (i.e. ft_transfer_call on intents.near). Thereby allowing an Agentic account to act autonomously on your behalf with restricted permissions and access only to the tokens that you transfer to your trading account. The system consists of two main contracts:
    1. Factory Contract (factory.rs):

    - Acts as a proxy contract deployer
    - Stores proxy code hash for verification
    - Creates proxy instances with proper initialization (see below for example)
    - Ensures secure deployment with minimum deposit requirements
    Example usage: `near call auth-v0.peerfolio.testnet deposit_and_create_proxy_global \
  '{"owner_id": "alice.testnet"}' \
  --accountId alice.testnet \
  --deposit 4`

    2. Auth Proxy Contract (auth_proxy.rs):

    - Manages authorized users for signature requests
    - Handles MPC signature generation for approved transactions
    - Restricts contract interactions to predefined set (wrap.near, intents.near)
    - Supports specific methods (near_deposit, add_public_key, etc.)
    Example usage: `near call alice.auth-v0.peerfolio.testnet  request_signature \
  '{...signature_args...}' \
  --accountId authorized-agent.testnet`

### Onboarding sequence

These are all the various components that interact during a user's onboarding.

  ```mermaid
sequenceDiagram
    autonumber
    participant Wallet as NEAR (& Wallet) <br> (crypto-native wallet<br>user.near)
    participant User as User <br> (Browser)
    participant ProxyFac as Proxy Factory <br>(ProxyFactory contract<br>auth-v0.peerfolio.near)
    participant TradingAcc as Proxy/Trading Account <br>(user.auth-v0.peerfolio.near)
    participant MPC as MPC Contract

    User->>Wallet: Connect wallet
    Wallet->>User: Function call key for <br> auth-v0.peerfolio.near <br> (limited access)
    critical Approve txn
        User->>Wallet: (deposit_and_create_proxy_global) <br> w/ 0.004 Ⓝ
    option no balance
        Wallet--xUser: TBD
    option timeout/browser window closed
        Wallet-->User: TBD
    end
    critical deposit_and_create_proxy_global()
        Wallet->>ProxyFac: deposit_and_create_proxy_global()
        ProxyFac->>TradingAcc: i. create proxy account<br>ii. transfer deposit<br>iii.deploy AuthProxy contract<br>iv. call AuthProxy.new to<br> set authorized user (user.near) <br> and MPC signer (v1.signer))
    option trading acc already exists
        ProxyFac-->User: error message
    option other error
        ProxyFac--xWallet: Refund 0.004 Ⓝ
    end

    critical MPC key registration
        User->>MPC: derive MPC public key for trading account
        User->>Wallet: Approve add MPC key + <br> authorized user (peerfolio.near) txn
        Wallet->>TradingAcc: MPC key with full access is set
    option service unavailable
        MPC--xUser: Retry flow
    option user rejects txn
        Wallet--xUser: error message
    end
  ```

Examples
  - Approve txn: https://testnet.nearblocks.io/txns/CF1ainGjroxtppNTWWkFgQsiC5kC4iJ3X7v8FgLMrWDE?tab=execution
  - deposit and create proxy: https://testnet.nearblocks.io/txns/CF1ainGjroxtppNTWWkFgQsiC5kC4iJ3X7v8FgLMrWDE?tab=execution#GV1aG4CqY6Lm2L28yb3tdinD2vZ7mZRS8fvPfBzahcnp
  - MPC key registration: https://testnet.nearblocks.io/txns/5Jyn459DhAaRxEqvTo3x724cgxCpjTH1Jaoc7uyVNQt9

### Agent execution sequence (swaps etc.)

This sequence shows how an agentic process uses the proxy trading account to autonomously execute transactions on behalf of the user.

```mermaid
sequenceDiagram
    autonumber
    participant Agent as Agentic Process <br> (authorized-agent.near)
    participant User as Trading Contract Owner <br> (user.near)
    participant Proxy as Proxy/Trading Account <br>(user.auth-v0.peerfolio.near)
    participant MPC as MPC Contract <br>(v1.signer-prod.near)
    participant Target as Target Contract <br>(wrap.near / intents.near)
    participant NEAR as NEAR Protocol

    Note over Agent,NEAR: Agent is pre-authorized by user via add_authorized_user()
    User->>Agent: User has already set the market conditions which should trigger the agentic actions.

    Agent->>Agent: Analyze market conditions<br/>Decide to execute trade
    Agent->>NEAR: Get latest block hash & nonce
    NEAR-->>Agent: block_hash, nonce

    Agent->>Proxy: request_signature(<br/>contract_id: "wrap.near",<br/>actions_json: '[{"type":"FunctionCall",...}]',<br/>nonce, block_hash,<br/>mpc_signer_pk, derivation_path)

    Note over Proxy: Validate: Is agent authorized?
    Proxy->>Proxy: Check authorized_users.contains(agent)

    Note over Proxy: Validate: Is action allowed?
    Proxy->>Proxy: validate_and_build_actions()<br/>✓ Contract in allowlist<br/>✓ Method in allowlist<br/>✓ Build OmniAction

    Proxy->>Proxy: Build transaction with<br/>TransactionBuilder

    Proxy->>Proxy: Hash transaction payload<br/>create_signature_request()

    Proxy->>MPC: sign(request: {<br/>  payload_v2: {ecdsa: hex_hash},<br/>  path: derivation_path,<br/>  domain_id: 0<br/>})

    Note over MPC: MPC generates ECDSA signature<br/>using derivation path

    MPC-->>Proxy: SignatureResponse {<br/>  big_r, s, recovery_id<br/>}

    Proxy->>Proxy: sign_request_callback()<br/>✓ Decode hex signature<br/>✓ Verify via ecrecover<br/>✓ Build signed transaction

    Proxy-->>Agent: base64(signed_transaction)

    Agent->>NEAR: broadcast_tx_commit(signed_tx)

    NEAR->>Target: Execute: near_deposit / swap / etc.

    Target-->>NEAR: Transaction result

    NEAR-->>Agent: tx_hash, status

    Note over Agent: Agent logs execution<br/>Updates strategy
```

**Key Security Features:**
- Agent must be pre-authorized via `add_authorized_user()`
- Only specific contracts allowed: `wrap.near`, `intents.near`
- Only specific methods allowed: `near_deposit`, `add_public_key`, etc.
- MPC signature provides cryptographic security
- All actions are logged on-chain for transparency

### Deleting a trading account

1. Add your main account public key to the proxy account with full access permissions
```
near contract call-function as-transaction <mainaccount>.auth-v0.peerfolio.testnet add_full_access_key json-args '{"public_key": "<main-account-public-key>"}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as <mainaccount>.testnet network-config testnet sign-with-keychain send
```

2. Send a delete account transaction signing with your main account private key
```
near account delete-account <mainaccount>.auth-v0.peerfolio.testnet beneficiary <mainaccount>.testnet network-config testnet sign-with-plaintext-private-key
```


#### Examples
1. [This testnet txn](https://testnet.nearblocks.io/txns/Hi2pfe89tBdMN2oY2dFXLuHcSBVFotx6pHViDQuKUZDi) converting 1 Near to WNear by `agent.charleslavon.testnet` was initiated by `benevio-labs.testnet` who was pre-approved by auth_proxy.rs to use Near's MPC contract to [create a signature](https://testnet.nearblocks.io/txns/831u2KqbdtzvJti5HUhGnp4tZD7Q8onUzD11rwBjrAAm).
2. [This mainnet txn](https://nearblocks.io/txns/GRw6oEWjAQ2QT9oDtsgBSRWr3s4oCW4A8zCpHCRXD62s) adding a public_key onto `intents.near` was initiated by `benevio-labs.near` who was pre-approved by auth_proxy.rs to [create an MPC signature](https://nearblocks.io/txns/9PJXbvcb4RMxjwK8VW4N54RnvrjENUCr6N1nv9f3DZJQ).


#### Setup dependencies
1. Install near-cli-rs
2. Set your target network as an environment variable e.g. `export NEAR_ENV=testnet`
3. Also add your factory account address and factory owner address into factory/deploy-factory.sh, e.g. `FACTORY_ACCOUNT="auth-v0.peerfolio.$NETWORK"
FACTORY_OWNER="peerfolio.$NETWORK"`
4. Login with a near testnet account and choose to save the private key into your mac's keychain, `near login`
5. Need tokens? Use a [Near testnet faucet](https://near-faucet.io/) to fund your account.
6. Build and install rust tooling

    ```bash
    # The repository pins Rust 1.85.0 for near-sandbox-compatible wasm.
    rustup toolchain install 1.85.0-aarch64-apple-darwin
    rustup target add wasm32-unknown-unknown --toolchain 1.85.0-aarch64-apple-darwin
    cd contracts && ./build_trading_account.sh
    cd factory && ./deploy-factory.sh
    ```

### Running tests

The test script builds the proxy wasm with `cargo near` before enabling the sandbox integration
tests. The test profile disables debug assertions because near-sdk's debug-only mock blockchain
check aborts on some host toolchains, while retaining panic unwinding for `#[should_panic]` tests.

```bash
cd contracts
./test.sh
```

#### Test Requesting Signatures
1. Go to [NearBlocks](https://testnet.nearblocks.io/), on the upper right select the `Near Icon`, then `testnet`, then click into a `Latest Block` and copy the block hash.  Now you can simulate a program or agent using your proxy contract by requesting a signature, `./request_signature.sh <block hash> < add_key | deposit > <your-other-account.testnet>`
2. If successful, transaction logs (view them in your terminal or on nearblocks.io) should display the Reconstructed Signature in base64 (scroll up or search for `Signed transaction (base64)`).  Pass this signature `./submit_txn.sh` to test broadcasting this testnet transaction. `./submit_txn.sh FAAAAGNoYXJsZXNsYXZvbi50ZXN0bmV0AQD1k+Pq3bhLFaNXClzgx0fEBmZItkkolypTJq0v0O6JOB856PxW5l+TZwD6MTrEBY+xsI/3wBgz2RNY+Ax5RETZq+2FlQEAAAwAAAB3cmFwLnRlc3RuZXQwRFzWCwWaY4pPFHl46Bj87dj6JLtdm28rjKf37iFc4QEAAAACDAAAAG5lYXJfZGVwb3NpdAAAAAAAoHJOGAkAAAAAAKHtzM4bwtMAAAAAAAAButebmlYXbKcuRM9NfWfgOAdR9jzGvS4Fv53T4/wOGjwwjizI0PvKnpaCpsxkNyTFZHQEVpYkCNPnUbabAYYx/QI=`


