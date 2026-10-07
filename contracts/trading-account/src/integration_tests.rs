use crate::test_support;
use anyhow::Result;
use near_workspaces::network::Sandbox;
use near_workspaces::types::{Gas, NearToken};
use near_workspaces::{Account, AccountId, Contract, DevNetwork, Worker, operations::Function};
use serde_json::json;

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

// Stands in for v1.signer: `set_response` stores the call's input, and `sign` returns it.
const STUB_SIGNER_WAT: &str = r#"(module
  (import "env" "input" (func $input (param i64)))
  (import "env" "register_len" (func $register_len (param i64) (result i64)))
  (import "env" "read_register" (func $read_register (param i64 i64)))
  (import "env" "storage_write" (func $storage_write (param i64 i64 i64 i64 i64) (result i64)))
  (import "env" "storage_read" (func $storage_read (param i64 i64 i64) (result i64)))
  (import "env" "value_return" (func $value_return (param i64 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "r")
  (func (export "set_response")
    (call $input (i64.const 0))
    (call $read_register (i64.const 0) (i64.const 1024))
    (drop (call $storage_write (i64.const 1) (i64.const 0)
      (call $register_len (i64.const 0)) (i64.const 1024) (i64.const 1))))
  (func (export "sign")
    (drop (call $storage_read (i64.const 1) (i64.const 0) (i64.const 0)))
    (call $read_register (i64.const 0) (i64.const 1024))
    (call $value_return (call $register_len (i64.const 0)) (i64.const 1024))))"#;

/// A trading account whose MPC signer is the stub, owned by itself (so `trading_account.call(...)`
/// calls as the owner), with an authorized agent. Returns the trading account, the stub signer and
/// the agent.
async fn deploy_with_stub_signer(
    worker: &Worker<impl DevNetwork>,
) -> Result<(Contract, Contract, Account)> {
    let signer = worker.dev_deploy(&wat::parse_str(STUB_SIGNER_WAT)?).await?;
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

// Stands in for the factory: get_proxy_code_base58_hash returns a fixed hash.
const STUB_FACTORY_WAT: &str = r#"(module
  (import "env" "value_return" (func $value_return (param i64 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "\"6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f\"")
  (func (export "get_proxy_code_base58_hash")
    (call $value_return (i64.const 46) (i64.const 0))))"#;

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
        .args_json(json!({ "expected_hash": "6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f" }))
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
    let stub_factory = wat::parse_str(STUB_FACTORY_WAT)?;
    let (trading_account, owner) = deploy_under_factory(&worker, Some(&stub_factory)).await?;
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

    let outcome = upgrade(
        &owner,
        &trading_account,
        "6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f",
    )
    .await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    assert_eq!(trading_account.view_state().await?, before);
    Ok(())
}
