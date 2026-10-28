// The restore that scripts/dev/restore.sh sends, rehearsed in the sandbox: a v0 account is
// flashed with the current code, then restored to its snapshot in one batch.
use super::*;
use near_primitives::transaction::{Action, DeployContractAction, FunctionCallAction};

const RESTORE_WASM: &[u8] =
    include_bytes!("../../../../tools/restore-state/target/near/restore_state.wasm");

fn deploy(code: &[u8]) -> Action {
    Action::DeployContract(DeployContractAction {
        code: code.to_vec(),
    })
}

fn call(method: &str, args: serde_json::Value) -> Action {
    Action::FunctionCall(Box::new(FunctionCallAction {
        method_name: method.to_string(),
        args: serde_json::to_vec(&args).unwrap(),
        gas: near_primitives::types::Gas::from_teragas(100),
        deposit: NearToken::from_near(0),
    }))
}

/// restore.sh's restore args: every snapshot entry as base64 [key, value], plus the keys
/// written since the snapshot, to remove.
fn restore_args(
    snapshot: &std::collections::HashMap<Vec<u8>, Vec<u8>>,
    now: &std::collections::HashMap<Vec<u8>, Vec<u8>>,
) -> serde_json::Value {
    use near_sdk::base64::{Engine, engine::general_purpose::STANDARD};
    let entries: Vec<_> = snapshot
        .iter()
        .map(|(k, v)| [STANDARD.encode(k), STANDARD.encode(v)])
        .collect();
    let remove: Vec<_> = now
        .keys()
        .filter(|k| !snapshot.contains_key(*k))
        .map(|k| STANDARD.encode(k))
        .collect();
    json!({ "entries": entries, "remove": remove })
}

/// A v0 account like the mainnet one: one agent, a full-access key the test signs with.
async fn v0_account(worker: &Worker<Sandbox>) -> Result<(FactoryAccount, Account)> {
    publish_global(worker, V0_WASM).await?;
    let account = create_factory_account(worker, V0_HASH).await?;
    account
        .owner
        .call(account.trading_account.id(), "add_authorized_user")
        .args_json(json!({ "account_id": "bot.peerfolio.near" }))
        .transact()
        .await?
        .into_result()?;
    let ta = add_temporary_key(worker, &account).await?;
    Ok((account, ta))
}

#[tokio::test]
async fn test_restore_returns_a_flashed_v0_account_to_its_snapshot() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (account, ta) = v0_account(&worker).await?;
    let id = ta.id().clone();
    let state_before = worker.view_state(&id).await?;
    let usage_before = worker.view_account(&id).await?.storage_usage;

    // Flash: deploy the current code and migrate, one transaction.
    let status = send_actions(
        &worker,
        &ta,
        &id,
        vec![deploy(TRADING_ACCOUNT_WASM), call("migrate", json!({}))],
    )
    .await?;
    anyhow::ensure!(succeeded(&status), "flash: {status}");
    let version: serde_json::Value = worker.view(&id, "contract_version").await?.json()?;
    assert_eq!(version["state_version"], 1);
    assert_eq!(worker.view_state(&id).await?[b"STATE".as_slice()][0], 1);

    // The owner becomes an agent and stays one: restore must remove its storage entries.
    account
        .owner
        .call(&id, "add_agent")
        .args_json(json!({ "account_id": account.owner.id() }))
        .transact()
        .await?
        .into_result()?;
    let args = restore_args(&state_before, &worker.view_state(&id).await?);
    assert!(!args["remove"].as_array().unwrap().is_empty());

    // Restore: restore helper, restore, v0 global code, one transaction.
    let status = send_actions(
        &worker,
        &ta,
        &id,
        vec![
            deploy(RESTORE_WASM),
            call("restore", args),
            use_global(V0_HASH),
        ],
    )
    .await?;
    anyhow::ensure!(succeeded(&status), "restore: {status}");

    assert_eq!(worker.view_state(&id).await?, state_before);
    assert_eq!(global_code_hash(&worker, &id).await?, V0_HASH);
    assert_eq!(worker.view_account(&id).await?.storage_usage, usage_before);
    let owner: AccountId = worker.view(&id, "get_owner_id").await?.json()?;
    assert_eq!(&owner, account.owner.id());
    assert!(worker.view(&id, "contract_version").await.is_err());
    Ok(())
}

// If restore fails, the whole batch reverts: the account stays on v1 with v1 state.
#[tokio::test]
async fn test_restore_that_fails_reverts_the_whole_batch() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (_account, ta) = v0_account(&worker).await?;
    let id = ta.id().clone();
    let status = send_actions(
        &worker,
        &ta,
        &id,
        vec![deploy(TRADING_ACCOUNT_WASM), call("migrate", json!({}))],
    )
    .await?;
    anyhow::ensure!(succeeded(&status), "flash: {status}");
    let state_v1 = worker.view_state(&id).await?;
    let code_v1 = worker.view_account(&id).await?.contract_state;

    let status = send_actions(
        &worker,
        &ta,
        &id,
        vec![
            deploy(RESTORE_WASM),
            call("restore", json!({ "entries": [["not base64!", ""]] })),
            use_global(V0_HASH),
        ],
    )
    .await?;

    assert!(!succeeded(&status), "{status}");
    assert_eq!(worker.view_state(&id).await?, state_v1);
    assert_eq!(worker.view_account(&id).await?.contract_state, code_v1);
    let version: serde_json::Value = worker.view(&id, "contract_version").await?.json()?;
    assert_eq!(version["contract_version"], "1.0.0");
    Ok(())
}
