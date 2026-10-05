use crate::test_support;
use anyhow::Result;
use near_workspaces::types::Gas;
use near_workspaces::{AccountId, Contract, DevNetwork, Worker, operations::Function};
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

async fn is_authorized(trading_account: &Contract, account_id: &AccountId) -> Result<bool> {
    Ok(trading_account
        .call("is_authorized")
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
    // is_authorized is also true for the owner.
    assert!(is_authorized(&trading_account, &owner_id).await?);
    Ok(())
}

#[tokio::test]
async fn test_add_and_remove_authorized_user() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let agent = worker.dev_create_account().await?;

    trading_account
        .call("add_authorized_user")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    assert!(is_authorized(&trading_account, agent.id()).await?);

    trading_account
        .call("remove_authorized_user")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    assert!(!is_authorized(&trading_account, agent.id()).await?);
    Ok(())
}

#[tokio::test]
async fn test_get_authorized_users() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let trading_account = deploy_trading_account(&worker).await?;
    let user1 = worker.dev_create_account().await?;
    let user2 = worker.dev_create_account().await?;

    trading_account
        .batch()
        .call(Function::new("add_authorized_user").args_json(json!({ "account_id": user1.id() })))
        .call(Function::new("add_authorized_user").args_json(json!({ "account_id": user2.id() })))
        .transact()
        .await?
        .into_result()?;

    let authorized_users: Vec<AccountId> =
        trading_account.view("get_authorized_users").await?.json()?;
    assert!(authorized_users.contains(user1.id()));
    assert!(authorized_users.contains(user2.id()));
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
            "contract_id": "wrap.testnet",
            "actions_json": r#"[{"type":"FunctionCall","method_name":"near_deposit","args":{},"gas":"30000000000000","deposit":"1"}]"#,
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
        failures.contains("Unauthorized: only authorized users can request signatures"),
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

// The full request_signature -> signer -> sign_request_callback path, with the minimum gas
// the docs promise is enough. The real signer's signature format is only checked on testnet.
// The deposits 1 and 10 are the pen test #8 case (ft-core#1700): the transaction JSON passed
// to the callback must carry both unchanged.
#[tokio::test]
async fn test_request_signature_with_stub_signer() -> Result<()> {
    use crate::TradingAccountContract;
    use near_sdk::{test_utils::VMContextBuilder, testing_env};

    let worker = near_workspaces::sandbox().await?;
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
        .call("add_authorized_user")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;

    let actions_json = r#"[
        {"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"},
        {"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"bob.near","token_id":"nep141:wrap.near","amount":"2000"},"gas":"30000000000000","deposit":"10"}
    ]"#;
    let nonce = 5;

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
    let (tx, _) = test_support::unsigned_tx(&mock, "intents.near", actions_json, nonce, [0u8; 32]);
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
            "block_hash": "11111111111111111111111111111111",
            "mpc_signer_pk": test_support::mpc_public_key(),
            "derivation_path": trading_account.id(),
        }))
        .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(outcome.is_success(), "{:#?}", outcome.failures());

    let signed = near_sdk::base64::Engine::decode(
        &near_sdk::base64::engine::general_purpose::STANDARD,
        outcome.json::<String>()?,
    )?;
    assert_eq!(signed, expected_signed);
    Ok(())
}
