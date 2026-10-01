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
}
