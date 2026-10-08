use crate::test_support;
use anyhow::Result;
use near_workspaces::network::Sandbox;
use near_workspaces::types::{Gas, KeyType, NearToken, SecretKey};
use near_workspaces::{
    Account, AccountId, Contract, ContractState, DevNetwork, Worker, operations::Function,
};
use serde_json::json;
use std::str::FromStr;

const TRADING_ACCOUNT_WASM: &[u8] =
    include_bytes!("../../target/near/trading_account/trading_account.wasm");

/// Deploys and initializes a trading account. Its owner is its own account, so
/// `trading_account.call(...)` calls as the owner.
async fn deploy_trading_account(worker: &Worker<impl DevNetwork>) -> Result<Contract> {
    let trading_account = worker.dev_deploy(TRADING_ACCOUNT_WASM).await?;
    trading_account
        .call("new")
        .args_json(json!({
            "owner_id": trading_account.id(),
            "signer_id": "v1.signer-prod.testnet",
        }))
        .transact()
        .await?
        .into_result()?;
    Ok(trading_account)
}

async fn is_agent(trading_account: &Contract, account_id: &AccountId) -> Result<bool> {
    Ok(trading_account
        .call("is_agent")
        .args_json(json!({ "account_id": account_id }))
        .view()
        .await?
        .json()?)
}

#[tokio::test]
async fn test_new_sets_owner() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;

    let owner_id: AccountId = trading_account.view("get_owner_id").await?.json()?;
    assert_eq!(&owner_id, trading_account.id());
    // The owner isn't an agent unless it adds itself.
    assert!(!is_agent(&trading_account, &owner_id).await?);
    Ok(())
}

// The methods were renamed to "agent" names in 1.0.0 with no aliases, so the old names must not exist.
#[tokio::test]
async fn test_authorized_user_method_names_are_gone() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let args = json!({ "account_id": trading_account.id() });

    for method in ["add_authorized_user", "remove_authorized_user"] {
        let result = trading_account
            .call(method)
            .args_json(args.clone())
            .transact()
            .await?;
        assert!(
            format!("{:?}", result.into_result()).contains("MethodNotFound"),
            "{method} should not exist"
        );
    }
    for method in ["is_authorized", "get_authorized_users"] {
        let result = trading_account.view(method).args_json(args.clone()).await;
        assert!(
            format!("{:?}", result).contains("MethodNotFound"),
            "{method} should not exist"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_add_and_remove_agent() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let agent = worker.dev_create_account().await?;

    trading_account
        .call("add_agent")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    assert!(is_agent(&trading_account, agent.id()).await?);

    trading_account
        .call("remove_agent")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    assert!(!is_agent(&trading_account, agent.id()).await?);
    Ok(())
}

#[tokio::test]
async fn test_get_agents() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let user1 = worker.dev_create_account().await?;
    let user2 = worker.dev_create_account().await?;

    trading_account
        .batch()
        .call(Function::new("add_agent").args_json(json!({ "account_id": user1.id() })))
        .call(Function::new("add_agent").args_json(json!({ "account_id": user2.id() })))
        .transact()
        .await?
        .into_result()?;

    let agents: Vec<AccountId> = trading_account.view("get_agents").await?.json()?;
    assert!(agents.contains(user1.id()));
    assert!(agents.contains(user2.id()));
    Ok(())
}

#[tokio::test]
async fn test_request_signature_rejects_unauthorized_caller() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let stranger = worker.dev_create_account().await?;

    // A valid request, so only the caller check can reject it.
    let outcome = stranger
        .call(trading_account.id(), "request_signature")
        .args_json(json!({
            "contract_id": "intents.near",
            "actions_json": r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#,
            "nonce": "1",
            "block_hash": "11111111111111111111111111111111",
            "mpc_signer_pk": test_support::mpc_public_key(),
            "derivation_path": trading_account.id(),
        }))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;

    assert!(outcome.is_failure());
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("Unauthorized: only agents can request signatures"),
        "{}",
        failures
    );
    Ok(())
}

/// A stub contract: `setter` stores the call's input, and `getter` returns it.
fn echo_stub(setter: &str, getter: &str) -> Result<Vec<u8>> {
    Ok(wat::parse_str(format!(
        r#"(module
  (import "env" "input" (func $input (param i64)))
  (import "env" "register_len" (func $register_len (param i64) (result i64)))
  (import "env" "read_register" (func $read_register (param i64 i64)))
  (import "env" "storage_write" (func $storage_write (param i64 i64 i64 i64 i64) (result i64)))
  (import "env" "storage_read" (func $storage_read (param i64 i64 i64) (result i64)))
  (import "env" "value_return" (func $value_return (param i64 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "k")
  (func (export "{setter}")
    (call $input (i64.const 0))
    (call $read_register (i64.const 0) (i64.const 1024))
    (drop (call $storage_write (i64.const 1) (i64.const 0)
      (call $register_len (i64.const 0)) (i64.const 1024) (i64.const 1))))
  (func (export "{getter}")
    (drop (call $storage_read (i64.const 1) (i64.const 0) (i64.const 0)))
    (call $read_register (i64.const 0) (i64.const 1024))
    (call $value_return (call $register_len (i64.const 0)) (i64.const 1024))))"#
    ))?)
}

/// Stands in for v1.signer: `set_response` primes what `sign` returns.
fn stub_signer() -> Result<Vec<u8>> {
    echo_stub("set_response", "sign")
}

/// A trading account whose MPC signer is the stub, owned by itself (so `trading_account.call(...)`
/// calls as the owner), with an authorized agent. Returns the trading account, the stub signer and
/// the agent.
async fn deploy_with_stub_signer(
    worker: &Worker<impl DevNetwork>,
) -> Result<(Contract, Contract, Account)> {
    let signer = worker.dev_deploy(&stub_signer()?).await?;
    let trading_account = worker.dev_deploy(TRADING_ACCOUNT_WASM).await?;
    trading_account
        .call("new")
        .args_json(json!({ "owner_id": trading_account.id(), "signer_id": signer.id() }))
        .transact()
        .await?
        .into_result()?;
    let agent = worker.dev_create_account().await?;
    trading_account
        .call("add_agent")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    Ok((trading_account, signer, agent))
}

/// Has `agent` request a signature for `actions_json` sent to intents.near, with the stub primed
/// to return the test MPC key's signature over the transaction the contract will build. Attaches
/// the documented minimum of 100 Tgas. Returns the base64 signed transaction and the bytes it
/// should decode to.
async fn request_signature_with_stub(
    trading_account: &Contract,
    signer: &Contract,
    agent: &Account,
    actions_json: &str,
    nonce: u64,
    block_hash: [u8; 32],
) -> Result<(String, Vec<u8>)> {
    use crate::TradingAccountContract;
    use near_sdk::{json_types::Base58CryptoHash, test_utils::VMContextBuilder, testing_env};

    // The transaction the contract will build, signed with the test MPC key.
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(trading_account.id().as_str().parse()?)
            .build()
    );
    let mock = TradingAccountContract::new(
        trading_account.id().as_str().parse()?,
        signer.id().as_str().parse()?,
    );
    let (tx, _) = test_support::unsigned_tx(&mock, "intents.near", actions_json, nonce, block_hash);
    let (response, expected_signed) = test_support::mpc_sign(&tx);
    signer
        .call("set_response")
        .args_json(response)
        .transact()
        .await?
        .into_result()?;

    let outcome = agent
        .call(trading_account.id(), "request_signature")
        .args_json(json!({
            "contract_id": "intents.near",
            "actions_json": actions_json,
            "nonce": nonce.to_string(),
            "block_hash": Base58CryptoHash::from(block_hash),
            "mpc_signer_pk": test_support::mpc_public_key(),
            "derivation_path": trading_account.id(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(outcome.is_success(), "{:#?}", outcome.failures());
    Ok((outcome.json()?, expected_signed))
}

// The full request_signature -> signer -> sign_request_callback path, with the minimum gas
// the docs promise is enough. The real signer's signature format is only checked on testnet.
// The deposits 1 and 10 are the pen test #8 case (ft-core#1700): the transaction JSON passed
// to the callback must carry both unchanged.
#[tokio::test]
async fn test_request_signature_with_stub_signer() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (trading_account, signer, agent) = deploy_with_stub_signer(&worker).await?;

    let actions_json = r#"[
        {"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"},
        {"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"bob.near","token_id":"nep141:wrap.near","amount":"2000"},"gas":"30000000000000","deposit":"10"}
    ]"#;
    let (signed_base64, expected_signed) = request_signature_with_stub(
        &trading_account,
        &signer,
        &agent,
        actions_json,
        5,
        [0u8; 32],
    )
    .await?;

    let signed = near_sdk::base64::Engine::decode(
        &near_sdk::base64::engine::general_purpose::STANDARD,
        signed_base64,
    )?;
    assert_eq!(signed, expected_signed);
    Ok(())
}

/// Broadcasts a base64 signed transaction with the sandbox's `send_tx` RPC and returns the
/// JSON-RPC response. near-workspaces only sends transactions it signs itself.
async fn broadcast(worker: &Worker<Sandbox>, signed_tx_base64: &str) -> Result<serde_json::Value> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": "send_tx",
        "params": { "signed_tx_base64": signed_tx_base64, "wait_until": "EXECUTED_OPTIMISTIC" },
    });
    let response = reqwest::Client::new()
        .post(worker.rpc_addr())
        .header("content-type", "application/json")
        .body(request.to_string())
        .send()
        .await?
        .text()
        .await?;
    Ok(serde_json::from_str(&response)?)
}

// Pen test findings #5 and #6 (ft-core#1791). A transaction the bot already had signed stays valid
// until it expires, about a day later, unless the MPC key is deleted. Two transactions are signed
// up front. The first is the control: broadcasting it works while the key exists. After the owner
// deletes the key, broadcasting the second fails with an unknown-key error, and adding the key
// back doesn't revive it. The second has the highest nonce the contract will sign, so no
// transaction it signed before the deletion can be revived.
#[tokio::test]
async fn test_delete_key_invalidates_signed_transactions() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (trading_account, signer, agent) = deploy_with_stub_signer(&worker).await?;
    let mpc_key = test_support::mpc_public_key();
    trading_account
        .call("add_full_access_key")
        .args_json(json!({ "public_key": mpc_key }))
        .transact()
        .await?
        .into_result()?;
    let nonce = worker
        .view_access_key(trading_account.id(), &mpc_key.parse()?)
        .await?
        .nonce;
    let block = worker.view_block().await?;
    let block_hash = block.hash().0;

    let actions_json = r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#;
    let (control, _) = request_signature_with_stub(
        &trading_account,
        &signer,
        &agent,
        actions_json,
        nonce + 1,
        block_hash,
    )
    .await?;
    let (hoarded, _) = request_signature_with_stub(
        &trading_account,
        &signer,
        &agent,
        actions_json,
        // The highest nonce the contract signs at this height.
        block.height() * 1_000_000 - 1,
        block_hash,
    )
    .await?;

    // Accepted. Its receipt then fails because the sandbox has no intents.near, which doesn't matter.
    let response = broadcast(&worker, &control).await?;
    assert!(response.get("error").is_none(), "{}", response);

    trading_account
        .call("delete_key")
        .args_json(json!({ "public_key": mpc_key }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let response = broadcast(&worker, &hoarded).await?;
    assert!(
        response.to_string().contains("AccessKeyNotFound"),
        "{}",
        response
    );

    // A re-added key starts at a nonce based on the current block height, above the hoarded one.
    trading_account
        .call("add_full_access_key")
        .args_json(json!({ "public_key": mpc_key }))
        .transact()
        .await?
        .into_result()?;
    let response = broadcast(&worker, &hoarded).await?;
    assert!(
        response.to_string().contains("InvalidNonce"),
        "{}",
        response
    );
    Ok(())
}

// A nonce for a future block height would make a transaction that becomes valid later, after the
// owner has deleted and re-added the MPC key. The contract refuses to sign it.
#[tokio::test]
async fn test_request_signature_rejects_nonce_from_a_future_block() -> Result<()> {
    use near_sdk::json_types::Base58CryptoHash;

    let worker = near_workspaces::sandbox().await?;
    let (trading_account, _signer, agent) = deploy_with_stub_signer(&worker).await?;
    let block = worker.view_block().await?;

    let outcome = agent
        .call(trading_account.id(), "request_signature")
        .args_json(json!({
            "contract_id": "intents.near",
            "actions_json": r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#,
            "nonce": ((block.height() + 30) * 1_000_000).to_string(),
            "block_hash": Base58CryptoHash::from(block.hash().0),
            "mpc_signer_pk": test_support::mpc_public_key(),
            "derivation_path": trading_account.id(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(outcome.is_failure());
    let failures = format!("{:?}", outcome.failures());
    assert!(failures.contains("Invalid nonce"), "{}", failures);
    Ok(())
}

// ---- Versioned state and migrate ----

#[tokio::test]
async fn test_contract_version_view() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;

    let version: serde_json::Value = trading_account.view("contract_version").await?.json()?;
    assert_eq!(
        version,
        json!({ "contract_version": "1.0.0", "state_version": 1 })
    );
    Ok(())
}

// migrate rewrites the whole state, so only the account itself may call it.
#[tokio::test]
async fn test_migrate_is_private() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let stranger = worker.dev_create_account().await?;

    let outcome = stranger
        .call(trading_account.id(), "migrate")
        .transact()
        .await?;

    assert!(outcome.is_failure());
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("Method migrate is private"),
        "{}",
        failures
    );
    Ok(())
}

// A redundant upgrade re-runs migrate at the current version, which must change nothing.
#[tokio::test]
async fn test_migrate_at_current_version_keeps_state() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let agent = worker.dev_create_account().await?;
    trading_account
        .call("add_agent")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    let before = trading_account.view_state().await?;

    let outcome = trading_account
        .call("migrate")
        .transact()
        .await?
        .into_result()?;

    assert_eq!(trading_account.view_state().await?, before);
    assert!(
        outcome
            .logs()
            .iter()
            .any(|log| log.starts_with("EVENT_JSON:")),
        "{:?}",
        outcome.logs()
    );
    assert!(is_agent(&trading_account, agent.id()).await?);
    Ok(())
}

// ---- upgrade and do_upgrade ----

/// Stands in for the factory: `set_hash` sets the JSON string `get_proxy_code_base58_hash` returns.
fn stub_factory() -> Result<Vec<u8>> {
    echo_stub("set_hash", "get_proxy_code_base58_hash")
}

/// A trading account created as a subaccount of a "factory" account running `factory_wasm` (no
/// contract if None), as the real factory creates them. Returns the trading account and its owner.
async fn deploy_under_factory(
    worker: &Worker<Sandbox>,
    factory_wasm: Option<&[u8]>,
) -> Result<(Contract, Account)> {
    let factory = worker
        .root_account()?
        .create_subaccount("factory")
        .initial_balance(NearToken::from_near(20))
        .transact()
        .await?
        .into_result()?;
    if let Some(wasm) = factory_wasm {
        factory.deploy(wasm).await?.into_result()?;
    }
    let trading_account = factory
        .create_subaccount("ta")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?
        .deploy(TRADING_ACCOUNT_WASM)
        .await?
        .into_result()?;
    let owner = worker.dev_create_account().await?;
    trading_account
        .call("new")
        .args_json(json!({ "owner_id": owner.id(), "signer_id": "v1.signer-prod.testnet" }))
        .transact()
        .await?
        .into_result()?;
    Ok((trading_account, owner))
}

/// `owner` calls upgrade(expected_hash) with the one yoctoNEAR it requires.
async fn upgrade(
    owner: &Account,
    trading_account: &Contract,
    expected_hash: &str,
) -> Result<near_workspaces::result::ExecutionFinalResult> {
    Ok(owner
        .call(trading_account.id(), "upgrade")
        .args_json(json!({ "expected_hash": expected_hash }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?)
}

// do_upgrade swaps the account's code, so only the account itself (via upgrade) may call it.
#[tokio::test]
async fn test_do_upgrade_is_private() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let stranger = worker.dev_create_account().await?;

    let outcome = stranger
        .call(trading_account.id(), "do_upgrade")
        .args_json(json!({ "expected_hash": V0_HASH }))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(outcome.is_failure());
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("Method do_upgrade is private"),
        "{}",
        failures
    );
    Ok(())
}

// A failure in do_upgrade, two receipts down, must fail the owner's transaction rather than
// report success. That holds only while every method in the chain returns its promise.
#[tokio::test]
async fn test_upgrade_fails_when_the_factory_points_elsewhere() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let stub_factory = stub_factory()?;
    let (trading_account, owner) = deploy_under_factory(&worker, Some(&stub_factory)).await?;
    point_factory_at(&worker, trading_account.id(), V0_HASH).await?;
    let before = trading_account.view_state().await?;

    let outcome = upgrade(&owner, &trading_account, "11111111111111111111111111111111").await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("factory pointer does not match expected_hash"),
        "{}",
        failures
    );
    assert_eq!(trading_account.view_state().await?, before);
    Ok(())
}

// If the factory can't answer, do_upgrade must fail too (#[callback_unwrap], not
// #[callback_result], which would swallow the error and report success).
#[tokio::test]
async fn test_upgrade_fails_when_the_factory_view_fails() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (trading_account, owner) = deploy_under_factory(&worker, None).await?;
    let before = trading_account.view_state().await?;

    let outcome = upgrade(&owner, &trading_account, V0_HASH).await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    assert_eq!(trading_account.view_state().await?, before);
    Ok(())
}

/// Points the stub factory above `trading_account_id` at `hash`.
async fn point_factory_at(
    worker: &Worker<Sandbox>,
    trading_account_id: &AccountId,
    hash: &str,
) -> Result<()> {
    let factory_id: AccountId = trading_account_id
        .get_parent_account_id()
        .expect("trading account has a parent")
        .into();
    worker
        .root_account()?
        .call(&factory_id, "set_hash")
        .args_json(hash)
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

// ---- Release gate ----
//
// Every state version still carried must survive this build's migrate (RFC "Release gate"). Each
// row starts from that version's released code, published as a global contract the way mainnet
// runs it, with an implicit owner, a non-default signer and all 10 agents. It upgrades to this
// build and checks they all come back and request_signature still works. One row per state
// version: v0 now. The v1 row starts from this build, because no versioned release exists yet;
// once 1.0.0 ships it starts from that release's wasm instead.

/// v0's code, fetched from mainnet by its global hash. It can't be rebuilt from source.
const V0_WASM: &[u8] = include_bytes!("../res/v0.wasm");
const V0_HASH: &str = "6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f";

/// The bs58 SHA-256 a global contract is published under.
fn code_hash(code: &[u8]) -> String {
    use sha2::Digest;
    bs58::encode(sha2::Sha256::digest(code)).into_string()
}

/// `code` with an empty custom section named "x" appended (id 0, 2 bytes, name length 1): the
/// same contract under a different hash, so an upgrade from `code` to it swaps the code for real.
fn rehashed(code: &[u8]) -> Vec<u8> {
    [code, &[0, 2, 1, b'x']].concat()
}

/// Signs `actions` from `signer` to `receiver_id` and sends them. near-workspaces can't send
/// global-contract actions. Returns the final outcome's `status`.
async fn send_actions(
    worker: &Worker<Sandbox>,
    signer: &Account,
    receiver_id: &AccountId,
    actions: Vec<near_primitives::transaction::Action>,
) -> Result<serde_json::Value> {
    use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};

    let secret_key: near_crypto::SecretKey = signer.secret_key().to_string().parse()?;
    let nonce = worker
        .view_access_key(signer.id(), &signer.secret_key().public_key())
        .await?
        .nonce;
    let tx = Transaction::V0(TransactionV0 {
        signer_id: signer.id().clone(),
        public_key: secret_key.public_key(),
        nonce: nonce + 1,
        receiver_id: receiver_id.clone(),
        block_hash: near_primitives::hash::CryptoHash(worker.view_block().await?.hash().0),
        actions,
    });
    let signature = secret_key.sign(tx.get_hash_and_size().0.as_ref());
    let signed = borsh::to_vec(&SignedTransaction::new(signature, tx))?;
    let response = broadcast(
        worker,
        &near_sdk::base64::Engine::encode(
            &near_sdk::base64::engine::general_purpose::STANDARD,
            signed,
        ),
    )
    .await?;
    anyhow::ensure!(response.get("error").is_none(), "{response}");
    Ok(response["result"]["status"].clone())
}

/// Publishes `code` as a global contract under its hash, and returns the hash.
async fn publish_global(worker: &Worker<Sandbox>, code: &[u8]) -> Result<String> {
    use near_primitives::action::{DeployGlobalContractAction, GlobalContractDeployMode};
    use near_primitives::transaction::Action;

    let root = worker.root_account()?;
    let status = send_actions(
        worker,
        &root,
        root.id(),
        vec![Action::DeployGlobalContract(DeployGlobalContractAction {
            code: code.to_vec().into(),
            deploy_mode: GlobalContractDeployMode::CodeHash,
        })],
    )
    .await?;
    anyhow::ensure!(status.get("SuccessValue").is_some(), "{status}");
    Ok(code_hash(code))
}

fn use_global(hash: &str) -> near_primitives::transaction::Action {
    use near_primitives::action::{GlobalContractIdentifier, UseGlobalContractAction};
    let hash = near_primitives::hash::CryptoHash::from_str(hash).expect("bs58 code hash");
    near_primitives::transaction::Action::UseGlobalContract(Box::new(UseGlobalContractAction {
        contract_identifier: GlobalContractIdentifier::CodeHash(hash),
    }))
}

fn migrate_call() -> near_primitives::transaction::Action {
    near_primitives::transaction::Action::FunctionCall(Box::new(
        near_primitives::transaction::FunctionCallAction {
            method_name: "migrate".to_string(),
            args: vec![],
            gas: near_primitives::types::Gas::from_gas(crate::MIGRATE_GAS.as_gas()),
            deposit: NearToken::from_near(0),
        },
    ))
}

/// A funded implicit account, the only kind of owner the factory accepts.
async fn create_implicit_owner(worker: &Worker<Sandbox>) -> Result<Account> {
    let key = SecretKey::from_random(KeyType::ED25519);
    let id: AccountId = hex::encode(key.public_key().key_data()).parse()?;
    worker
        .root_account()?
        .transfer_near(&id, NearToken::from_near(10))
        .await?
        .into_result()?;
    Ok(Account::from_secret_key(id, key, worker))
}

/// The accounts of one release-gate row.
struct Row {
    trading_account: Contract,
    owner: Account,
    signer: Contract,
    /// The one agent with an account; the other nine are ids only.
    agent: Account,
    agents: Vec<AccountId>,
}

/// A trading account running the global contract `hash`, created under a stub factory the way
/// `create_proxy_global` creates it: one batch that creates the account, switches it to the
/// global code and calls `new`, leaving it with no keys. The owner then adds 10 agents with
/// `add_agent_method` (`add_authorized_user` in v0).
async fn create_row(worker: &Worker<Sandbox>, hash: &str, add_agent_method: &str) -> Result<Row> {
    use near_primitives::transaction::{
        Action, CreateAccountAction, FunctionCallAction, TransferAction,
    };

    let factory = worker
        .root_account()?
        .create_subaccount("factory")
        .initial_balance(NearToken::from_near(50))
        .transact()
        .await?
        .into_result()?;
    factory.deploy(&stub_factory()?).await?.into_result()?;
    let owner = create_implicit_owner(worker).await?;
    let signer = worker.dev_deploy(&stub_signer()?).await?;

    let id: AccountId = format!("ta.{}", factory.id()).parse()?;
    let status = send_actions(
        worker,
        &factory,
        &id,
        vec![
            Action::CreateAccount(CreateAccountAction {}),
            Action::Transfer(TransferAction {
                deposit: NearToken::from_near(10),
            }),
            use_global(hash),
            Action::FunctionCall(Box::new(FunctionCallAction {
                method_name: "new".to_string(),
                args: serde_json::to_vec(
                    &json!({ "owner_id": owner.id(), "signer_id": signer.id() }),
                )?,
                gas: near_primitives::types::Gas::from_teragas(50),
                deposit: NearToken::from_near(0),
            })),
        ],
    )
    .await?;
    anyhow::ensure!(status.get("SuccessValue").is_some(), "{status}");
    // The account holds no keys; this one only names it for views and calls made by others.
    let trading_account =
        Contract::from_secret_key(id, SecretKey::from_random(KeyType::ED25519), worker);

    let agent = worker.dev_create_account().await?;
    let mut agents = vec![agent.id().clone()];
    agents.extend((1..10).map(|i| format!("agent{i}.test.near").parse().unwrap()));
    for agent_id in &agents {
        owner
            .call(trading_account.id(), add_agent_method)
            .args_json(json!({ "account_id": agent_id }))
            .transact()
            .await?
            .into_result()?;
    }
    Ok(Row {
        trading_account,
        owner,
        signer,
        agent,
        agents,
    })
}

async fn global_code_hash(worker: &Worker<Sandbox>, account_id: &AccountId) -> Result<String> {
    match worker.view_account(account_id).await?.contract_state {
        ContractState::GlobalHash(hash) => Ok(hash.to_string()),
        other => anyhow::bail!("not on a global contract: {other:?}"),
    }
}

/// Asserts `row`'s trading account came through the upgrade whole: this build's version, the same
/// owner, signer and 10 agents, and a working request_signature.
async fn assert_row_survived(row: &Row) -> Result<()> {
    let ta = &row.trading_account;
    let version: serde_json::Value = ta.view("contract_version").await?.json()?;
    assert_eq!(
        version,
        json!({ "contract_version": crate::CONTRACT_VERSION, "state_version": crate::STATE_VERSION })
    );
    assert_eq!(
        ta.view("get_owner_id").await?.json::<AccountId>()?,
        *row.owner.id()
    );
    assert_eq!(
        ta.view("get_signer_id").await?.json::<AccountId>()?,
        *row.signer.id()
    );
    let mut agents: Vec<AccountId> = ta.view("get_agents").await?.json()?;
    agents.sort();
    let mut expected = row.agents.clone();
    expected.sort();
    assert_eq!(agents, expected);
    request_signature_with_stub(
        ta,
        &row.signer,
        &row.agent,
        r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#,
        1,
        [0u8; 32],
    )
    .await?;
    Ok(())
}

/// Adds a temporary full-access key to a v0 account through v0's `add_full_access_key`, as step 2
/// of the bootstrap does. Returns the account signing with that key.
async fn add_temporary_key(worker: &Worker<Sandbox>, row: &Row) -> Result<Account> {
    let key = SecretKey::from_random(KeyType::ED25519);
    row.owner
        .call(row.trading_account.id(), "add_full_access_key")
        .args_json(json!({ "public_key": key.public_key() }))
        .transact()
        .await?
        .into_result()?;
    Ok(Account::from_secret_key(
        row.trading_account.id().clone(),
        key,
        worker,
    ))
}

/// Step 3 of the bootstrap: the temporary key switches the account to `hash`, migrates it and
/// deletes itself, in one batch. Returns the final status.
async fn send_bootstrap_batch(
    worker: &Worker<Sandbox>,
    temporary: &Account,
    hash: &str,
) -> Result<serde_json::Value> {
    use near_primitives::transaction::{Action, DeleteKeyAction};
    send_actions(
        worker,
        temporary,
        temporary.id(),
        vec![
            use_global(hash),
            migrate_call(),
            Action::DeleteKey(Box::new(DeleteKeyAction {
                public_key: temporary.secret_key().public_key().into(),
            })),
        ],
    )
    .await
}

/// Rewrites the account's state version byte to one newer than this build knows, so this build's
/// migrate refuses it as a downgrade. Returns the patched STATE bytes.
async fn patch_future_state_version(
    worker: &Worker<Sandbox>,
    account_id: &AccountId,
) -> Result<Vec<u8>> {
    let mut state = worker
        .view_state(account_id)
        .await?
        .remove(b"STATE".as_slice())
        .expect("STATE");
    state[0] = crate::STATE_VERSION + 1;
    worker.patch_state(account_id, b"STATE", &state).await?;
    Ok(state)
}

#[test]
fn test_v0_fixture_is_the_mainnet_code() {
    assert_eq!(code_hash(V0_WASM), V0_HASH);
}

#[tokio::test]
async fn test_v0_bootstrap_keeps_owner_signer_and_agents() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let v0 = publish_global(&worker, V0_WASM).await?;
    let candidate = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let row = create_row(&worker, &v0, "add_authorized_user").await?;
    let temporary = add_temporary_key(&worker, &row).await?;

    let status = send_bootstrap_batch(&worker, &temporary, &candidate).await?;

    assert!(status.get("SuccessValue").is_some(), "{status}");
    assert_eq!(
        global_code_hash(&worker, row.trading_account.id()).await?,
        candidate
    );
    assert!(
        worker
            .view_access_key(
                row.trading_account.id(),
                &temporary.secret_key().public_key()
            )
            .await
            .is_err(),
        "temporary key survived the bootstrap"
    );
    assert_row_survived(&row).await
}

// The code swap, migrate and the key deletion share one receipt, so a migrate panic undoes all
// three, and the temporary key is still there to retry with.
#[tokio::test]
async fn test_v0_bootstrap_reverts_when_migrate_panics() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let v0 = publish_global(&worker, V0_WASM).await?;
    let candidate = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let row = create_row(&worker, &v0, "add_authorized_user").await?;
    let temporary = add_temporary_key(&worker, &row).await?;
    let state = patch_future_state_version(&worker, row.trading_account.id()).await?;

    let status = send_bootstrap_batch(&worker, &temporary, &candidate).await?;

    assert!(
        status.to_string().contains("downgrade not supported"),
        "{status}"
    );
    assert_eq!(
        global_code_hash(&worker, row.trading_account.id()).await?,
        v0
    );
    assert_eq!(
        row.trading_account.view_state().await?[b"STATE".as_slice()],
        state
    );
    worker
        .view_access_key(
            row.trading_account.id(),
            &temporary.secret_key().public_key(),
        )
        .await?;
    Ok(())
}

// The v1 row, through upgrade with the documented 100 Tgas. Upgrading again once at latest swaps
// to the same code and changes nothing.
#[tokio::test]
async fn test_upgrade_swaps_code_and_keeps_owner_signer_and_agents() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let current = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let candidate = publish_global(&worker, &rehashed(TRADING_ACCOUNT_WASM)).await?;
    let row = create_row(&worker, &current, "add_agent").await?;
    point_factory_at(&worker, row.trading_account.id(), &candidate).await?;
    let state = row.trading_account.view_state().await?;

    for _ in 0..2 {
        row.owner
            .call(row.trading_account.id(), "upgrade")
            .args_json(json!({ "expected_hash": candidate }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?
            .into_result()?;

        assert_eq!(
            global_code_hash(&worker, row.trading_account.id()).await?,
            candidate
        );
        assert_eq!(row.trading_account.view_state().await?, state);
    }
    assert_row_survived(&row).await
}

// The versioned flow's batch is one receipt too, so a migrate panic leaves the old code running
// on the old state, and the owner's transaction fails.
#[tokio::test]
async fn test_upgrade_reverts_when_migrate_panics() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let current = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let candidate = publish_global(&worker, &rehashed(TRADING_ACCOUNT_WASM)).await?;
    let row = create_row(&worker, &current, "add_agent").await?;
    point_factory_at(&worker, row.trading_account.id(), &candidate).await?;
    let state = patch_future_state_version(&worker, row.trading_account.id()).await?;

    let outcome = upgrade(&row.owner, &row.trading_account, &candidate).await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    let failures = format!("{:?}", outcome.failures());
    assert!(failures.contains("downgrade not supported"), "{}", failures);
    assert_eq!(
        global_code_hash(&worker, row.trading_account.id()).await?,
        current
    );
    assert_eq!(
        row.trading_account.view_state().await?[b"STATE".as_slice()],
        state
    );
    Ok(())
}
