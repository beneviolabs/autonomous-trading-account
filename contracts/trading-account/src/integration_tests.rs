#[cfg(test)]
mod contract_tests {

    use anyhow::Result;
    use near_sdk::AccountId;
    use near_workspaces::{Account, Contract, DevNetwork, Worker, operations::Function};
    use serde_json::json;

    const WASM_FILEPATH: &[u8] =
        include_bytes!("../../target/near/trading_account/trading_account.wasm");

    async fn init(worker: &Worker<impl DevNetwork>) -> Result<(Contract, Account)> {
        let trading_account = worker.dev_deploy(WASM_FILEPATH).await?;
        let owner = trading_account.as_account();

        // Initialize the contract
        let _result = trading_account
            .call("new")
            .args_json(json!({
                "owner_id": owner.id(),
                "signer_id": AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap()
            }))
            .transact()
            .await?;

        Ok((trading_account.clone(), owner.clone()))
    }

    #[tokio::test]
    async fn trading_account_initialization() -> Result<()> {
        let worker = near_workspaces::sandbox().await?;
        let (contract, owner) = init(&worker).await?;

        let contract_owner = contract
            .call("get_owner_id")
            .view()
            .await?
            .json::<String>()?;

        assert_eq!(
            contract_owner,
            owner.id().to_string(),
            "Contract owner should match"
        );

        // Test owner authorization
        let result = contract
            .call("is_authorized")
            .args_json(json!({
                "account_id": owner.id()
            }))
            .view()
            .await?
            .json::<bool>()?;

        assert!(result, "Owner should be authorized");

        Ok(())
    }

    #[tokio::test]
    async fn test_add_authorized_user() -> Result<()> {
        let worker = near_workspaces::sandbox().await?;
        let (contract, _owner) = init(&worker).await?;

        // Create a new account to authorize
        let new_user = worker.dev_create_account().await?;

        // Add new user as authorized user
        let _ = contract
            .call("add_authorized_user")
            .args_json(json!({
                "account_id": new_user.id()
            }))
            .transact()
            .await?;

        // Verify the user is authorized
        let is_authorized = contract
            .call("is_authorized")
            .args_json(json!({
                "account_id": new_user.id()
            }))
            .view()
            .await?
            .json::<bool>()?;

        assert!(is_authorized, "New user should be authorized");
        Ok(())
    }

    #[tokio::test]
    async fn test_remove_authorized_user() -> Result<()> {
        let worker = near_workspaces::sandbox().await?;
        let (contract, _owner) = init(&worker).await?;

        // Create and authorize a new user
        let user = worker.dev_create_account().await?;
        let _ = contract
            .call("add_authorized_user")
            .args_json(json!({
                "account_id": user.id()
            }))
            .transact()
            .await?;

        // Remove authorization
        let _ = contract
            .call("remove_authorized_user")
            .args_json(json!({
                "account_id": user.id()
            }))
            .transact()
            .await?;

        // Verify user is no longer authorized
        let is_authorized = contract
            .call("is_authorized")
            .args_json(json!({
                "account_id": user.id()
            }))
            .view()
            .await?
            .json::<bool>()?;

        assert!(!is_authorized, "User should no longer be authorized");
        Ok(())
    }

    #[tokio::test]
    async fn test_request_signature_unauthorized() -> Result<()> {
        let worker = near_workspaces::sandbox().await?;
        let (contract, _) = init(&worker).await?;

        // Create unauthorized user
        let unauthorized_user = worker.dev_create_account().await?;

        // Attempt signature request as unauthorized user
        let result = unauthorized_user
            .call(contract.id(), "request_signature")
            .args_json(json!({
                "contract_id": "wrap.testnet",
                "actions_json": "[{\"type\":\"FunctionCall\", \"deposit\": \"50000000000000000000000\", \"gas\": \"300000000000000\", \"method_name\": \"near_deposit\", \"args\": \"\"}]",
                "nonce": "1",
                // bs58 for 32 zero bytes, the block hash unsigned_tx used above.
                "block_hash": "11111111111111111111111111111111",
                "mpc_signer_pk":"ed25519:asdf".to_string(),
                "derivation_path": "agent.auth-factory.appaccount.testnet".to_string(),

            }))
            .gas(near_workspaces::types::Gas::from_tgas(200))
            .transact()
            .await;

        println!("Result: {:?}", result);
        // Check status before unwrapping
        let is_ok = result.is_ok();
        // Unwrap the error since we expect this to fail
        let final_result = result.unwrap();
        assert!(is_ok);
        assert!(final_result.is_failure());
        let err_msg = format!("{:?}", final_result.failures());
        assert!(
            err_msg.contains("Unauthorized: only authorized users can request signatures"),
            "Expected 'Unauthorized:...' error, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_authorized_users() -> Result<()> {
        let worker = near_workspaces::sandbox().await?;
        let (contract, _owner) = init(&worker).await?;

        // Add multiple users
        let user1 = worker.dev_create_account().await?;
        let user2 = worker.dev_create_account().await?;

        let _ = contract
            .batch()
            .call(
                Function::new("add_authorized_user").args_json(json!({ "account_id": user1.id() })),
            )
            .call(
                Function::new("add_authorized_user").args_json(json!({ "account_id": user2.id() })),
            )
            .transact()
            .await?;

        // Get all authorized users
        let authorized_users = contract
            .call("get_authorized_users")
            .view()
            .await?
            .json::<Vec<String>>()?;

        assert!(authorized_users.contains(&user1.id().to_string()));
        assert!(authorized_users.contains(&user2.id().to_string()));
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
    #[tokio::test]
    async fn test_request_signature_with_stub_signer() -> Result<()> {
        use crate::{TradingAccountContract, test_support};
        use near_sdk::{test_utils::VMContextBuilder, testing_env};

        let worker = near_workspaces::sandbox().await?;
        let signer = worker.dev_deploy(&wat::parse_str(STUB_SIGNER_WAT)?).await?;
        let trading_account = worker.dev_deploy(WASM_FILEPATH).await?;
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

        let actions_json = r#"[{"type":"FunctionCall","method_name":"near_deposit","args":{},"gas":"30000000000000","deposit":"50000000000000000000000"}]"#;
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
        let (tx, _) = test_support::unsigned_tx(&mock, "wrap.near", actions_json, nonce, [0u8; 32]);
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
                "contract_id": "wrap.near",
                "actions_json": actions_json,
                "nonce": nonce.to_string(),
                "block_hash": "11111111111111111111111111111111",
                "mpc_signer_pk": test_support::mpc_public_key(),
                "derivation_path": trading_account.id(),
            }))
            .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
            .gas(near_workspaces::types::Gas::from_tgas(100))
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
}
