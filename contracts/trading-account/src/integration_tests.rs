use anyhow::Result;
use near_workspaces::network::Sandbox;
use near_workspaces::types::{AccessKey, AccountDetailsPatch, Gas, KeyType, NearToken, SecretKey};
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
        .args_json(json!({ "owner_id": trading_account.id() }))
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

// ---- Accounts as the factory creates them ----
//
// The upgrade tests use trading accounts set up the way mainnet runs them: the code is a published
// global contract, and the account is a subaccount of a stub factory, which is where upgrade reads
// the latest hash from. near-workspaces can't send global-contract actions, so these transactions
// are built and signed here.

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

/// Signs `actions` from `signer` to `receiver_id` and sends them. Returns the final outcome's
/// `status`, which is `{"SuccessValue": …}` or `{"Failure": …}`.
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

fn succeeded(status: &serde_json::Value) -> bool {
    status.get("SuccessValue").is_some()
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
    anyhow::ensure!(succeeded(&status), "{status}");
    Ok(code_hash(code))
}

/// The action that switches an account to the global contract `hash`.
fn use_global(hash: &str) -> near_primitives::transaction::Action {
    use near_primitives::action::{GlobalContractIdentifier, UseGlobalContractAction};
    let hash = near_primitives::hash::CryptoHash::from_str(hash).expect("bs58 code hash");
    near_primitives::transaction::Action::UseGlobalContract(Box::new(UseGlobalContractAction {
        contract_identifier: GlobalContractIdentifier::CodeHash(hash),
    }))
}

/// Stands in for the factory: `set_hash` sets the JSON string `get_proxy_code_base58_hash` returns.
/// Until it's set, `get_proxy_code_base58_hash` fails.
fn stub_factory() -> Result<Vec<u8>> {
    echo_stub("set_hash", "get_proxy_code_base58_hash")
}

/// A trading account created by the stub factory, with its owner.
struct FactoryAccount {
    trading_account: Contract,
    owner: Account,
}

/// Creates a trading account running the global contract `hash`, the way `create_proxy_global`
/// does: one batch from the factory that creates the account, switches it to the global code and
/// calls `new`. The owner is an implicit account, the only kind the factory accepts.
async fn create_factory_account(worker: &Worker<Sandbox>, hash: &str) -> Result<FactoryAccount> {
    use near_primitives::transaction::{
        Action, CreateAccountAction, FunctionCallAction, TransferAction,
    };

    let root = worker.root_account()?;
    let factory = root
        .create_subaccount("factory")
        .initial_balance(NearToken::from_near(50))
        .transact()
        .await?
        .into_result()?;
    factory.deploy(&stub_factory()?).await?.into_result()?;
    let owner_key = SecretKey::from_random(KeyType::ED25519);
    let owner_id: AccountId = hex::encode(owner_key.public_key().key_data()).parse()?;
    root.transfer_near(&owner_id, NearToken::from_near(10))
        .await?
        .into_result()?;
    let owner = Account::from_secret_key(owner_id, owner_key, worker);

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
                // v0's new takes signer_id. This build's ignores it, as it must while the deployed
                // factory still sends it.
                args: serde_json::to_vec(
                    &json!({ "owner_id": owner.id(), "signer_id": "v1.signer" }),
                )?,
                gas: near_primitives::types::Gas::from_teragas(50),
                deposit: NearToken::from_near(0),
            })),
        ],
    )
    .await?;
    anyhow::ensure!(succeeded(&status), "{status}");
    // Like on mainnet, the account holds no keys. This random one only names it, for views and
    // for calls other accounts make to it.
    let trading_account =
        Contract::from_secret_key(id, SecretKey::from_random(KeyType::ED25519), worker);
    Ok(FactoryAccount {
        trading_account,
        owner,
    })
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

/// The bs58 hash of the global contract the account runs.
async fn global_code_hash(worker: &Worker<Sandbox>, account_id: &AccountId) -> Result<String> {
    match worker.view_account(account_id).await?.contract_state {
        ContractState::GlobalHash(hash) => Ok(hash.to_string()),
        other => anyhow::bail!("not on a global contract: {other:?}"),
    }
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

// ---- upgrade and do_upgrade ----

/// `account`'s owner calls upgrade(expected_hash) with the one yoctoNEAR it requires and the
/// 100 Tgas docs/reference.md tells callers to attach.
async fn upgrade(
    account: &FactoryAccount,
    expected_hash: &str,
) -> Result<near_workspaces::result::ExecutionFinalResult> {
    Ok(account
        .owner
        .call(account.trading_account.id(), "upgrade")
        .args_json(json!({ "expected_hash": expected_hash }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
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
    let current = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let account = create_factory_account(&worker, &current).await?;
    point_factory_at(&worker, account.trading_account.id(), V0_HASH).await?;
    let before = account.trading_account.view_state().await?;

    let outcome = upgrade(&account, "11111111111111111111111111111111").await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("factory pointer does not match expected_hash"),
        "{}",
        failures
    );
    assert_eq!(account.trading_account.view_state().await?, before);
    Ok(())
}

// If the factory can't answer, do_upgrade must fail too (#[callback_unwrap], not
// #[callback_result], which would swallow the error and report success).
#[tokio::test]
async fn test_upgrade_fails_when_the_factory_view_fails() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let current = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    // The stub factory's hash is never set, so its view fails.
    let account = create_factory_account(&worker, &current).await?;
    let before = account.trading_account.view_state().await?;

    let outcome = upgrade(&account, V0_HASH).await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    assert_eq!(account.trading_account.view_state().await?, before);
    Ok(())
}

// The code swap and migrate share one receipt, so a migrate panic leaves the old code running on
// the old state, and the owner's transaction fails.
#[tokio::test]
async fn test_upgrade_reverts_when_migrate_panics() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let current = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let candidate = publish_global(&worker, &rehashed(TRADING_ACCOUNT_WASM)).await?;
    let account = create_factory_account(&worker, &current).await?;
    point_factory_at(&worker, account.trading_account.id(), &candidate).await?;
    let state = patch_future_state_version(&worker, account.trading_account.id()).await?;

    let outcome = upgrade(&account, &candidate).await?;

    assert!(outcome.is_failure(), "{:?}", outcome);
    let failures = format!("{:?}", outcome.failures());
    assert!(failures.contains("downgrade not supported"), "{}", failures);
    assert_eq!(
        global_code_hash(&worker, account.trading_account.id()).await?,
        current
    );
    assert_eq!(
        account.trading_account.view_state().await?[b"STATE".as_slice()],
        state
    );
    Ok(())
}

// ---- Release gate ----
//
// Every state version still carried must survive this build's migrate (RFC "Release gate"). Each
// row starts from that version's released code, adds all 10 agents, upgrades to this build, and
// checks the owner and every agent come back and an agent can still swap. One row per state
// version:
// - v0: the mainnet v0 wasm, upgraded through the bootstrap batch.
// - v1: starts from this build, because no versioned release exists yet. Once 1.0.0 ships, it
//   starts from that release's wasm instead.

/// Has the owner add 10 agents with `method` (`add_authorized_user` in v0). Returns the one agent
/// with an account, which can call swap, and all 10 ids.
async fn add_ten_agents(
    worker: &Worker<Sandbox>,
    account: &FactoryAccount,
    method: &str,
) -> Result<(Account, Vec<AccountId>)> {
    let agent = worker.dev_create_account().await?;
    let mut agents = vec![agent.id().clone()];
    agents.extend((1..10).map(|i| format!("agent{i}.test.near").parse().unwrap()));
    for agent_id in &agents {
        account
            .owner
            .call(account.trading_account.id(), method)
            .args_json(json!({ "account_id": agent_id }))
            .transact()
            .await?
            .into_result()?;
    }
    Ok((agent, agents))
}

/// Asserts the account came through the upgrade whole: this build's versions, the same owner and
/// agents, and `agent` still allowed to swap.
async fn assert_upgraded_intact(
    account: &FactoryAccount,
    agent: &Account,
    agents: &[AccountId],
) -> Result<()> {
    let ta = &account.trading_account;
    let version: serde_json::Value = ta.view("contract_version").await?.json()?;
    assert_eq!(
        version,
        json!({ "contract_version": crate::CONTRACT_VERSION, "state_version": crate::STATE_VERSION })
    );
    assert_eq!(
        ta.view("get_owner_id").await?.json::<AccountId>()?,
        *account.owner.id()
    );
    let mut got: Vec<AccountId> = ta.view("get_agents").await?.json()?;
    got.sort();
    let mut expected = agents.to_vec();
    expected.sort();
    assert_eq!(got, expected);
    // The quote pays out to another account, so swap gets past the caller check and fails on the
    // recipient. A dropped agent would fail on the caller check instead.
    let quote = test_quote(json!({ "recipient": "attacker.near" }));
    let outcome = swap(agent, ta, &quote, Gas::from_tgas(100)).await?;
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("is not the trading account or its owner"),
        "{}",
        failures
    );
    Ok(())
}

/// Bootstrap step 2: the owner adds a temporary full-access key with v0's `add_full_access_key`.
/// Returns the trading account, signing with that key.
async fn add_temporary_key(worker: &Worker<Sandbox>, account: &FactoryAccount) -> Result<Account> {
    let key = SecretKey::from_random(KeyType::ED25519);
    account
        .owner
        .call(account.trading_account.id(), "add_full_access_key")
        .args_json(json!({ "public_key": key.public_key() }))
        .transact()
        .await?
        .into_result()?;
    Ok(Account::from_secret_key(
        account.trading_account.id().clone(),
        key,
        worker,
    ))
}

/// Bootstrap step 3: the temporary key switches the account to `hash`, migrates it and deletes
/// itself, in one batch. Returns the final status.
async fn send_bootstrap_batch(
    worker: &Worker<Sandbox>,
    temporary: &Account,
    hash: &str,
) -> Result<serde_json::Value> {
    use near_primitives::transaction::{Action, DeleteKeyAction, FunctionCallAction};
    send_actions(
        worker,
        temporary,
        temporary.id(),
        vec![
            use_global(hash),
            Action::FunctionCall(Box::new(FunctionCallAction {
                method_name: "migrate".to_string(),
                args: vec![],
                gas: near_primitives::types::Gas::from_gas(crate::MIGRATE_GAS.as_gas()),
                deposit: NearToken::from_near(0),
            })),
            Action::DeleteKey(Box::new(DeleteKeyAction {
                public_key: temporary.secret_key().public_key().into(),
            })),
        ],
    )
    .await
}

#[tokio::test]
async fn test_v0_bootstrap_keeps_owner_and_agents() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let v0 = publish_global(&worker, V0_WASM).await?;
    let candidate = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let account = create_factory_account(&worker, &v0).await?;
    let (agent, agents) = add_ten_agents(&worker, &account, "add_authorized_user").await?;
    let temporary = add_temporary_key(&worker, &account).await?;

    let status = send_bootstrap_batch(&worker, &temporary, &candidate).await?;

    assert!(succeeded(&status), "{status}");
    assert_eq!(
        global_code_hash(&worker, account.trading_account.id()).await?,
        candidate
    );
    assert!(
        worker
            .view_access_key(
                account.trading_account.id(),
                &temporary.secret_key().public_key()
            )
            .await
            .is_err(),
        "temporary key survived the bootstrap"
    );
    assert_upgraded_intact(&account, &agent, &agents).await
}

// The code swap, migrate and the key deletion share one receipt, so a migrate panic undoes all
// three, and the temporary key is still there to retry with.
#[tokio::test]
async fn test_v0_bootstrap_reverts_when_migrate_panics() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let v0 = publish_global(&worker, V0_WASM).await?;
    let candidate = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let account = create_factory_account(&worker, &v0).await?;
    let temporary = add_temporary_key(&worker, &account).await?;
    let state = patch_future_state_version(&worker, account.trading_account.id()).await?;

    let status = send_bootstrap_batch(&worker, &temporary, &candidate).await?;

    assert!(
        status.to_string().contains("downgrade not supported"),
        "{status}"
    );
    assert_eq!(
        global_code_hash(&worker, account.trading_account.id()).await?,
        v0
    );
    assert_eq!(
        account.trading_account.view_state().await?[b"STATE".as_slice()],
        state
    );
    worker
        .view_access_key(
            account.trading_account.id(),
            &temporary.secret_key().public_key(),
        )
        .await?;
    Ok(())
}

// The v1 row. Upgrading a second time, once already at latest, swaps to the same code and
// changes nothing.
#[tokio::test]
async fn test_upgrade_keeps_owner_and_agents() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let current = publish_global(&worker, TRADING_ACCOUNT_WASM).await?;
    let candidate = publish_global(&worker, &rehashed(TRADING_ACCOUNT_WASM)).await?;
    let account = create_factory_account(&worker, &current).await?;
    let (agent, agents) = add_ten_agents(&worker, &account, "add_agent").await?;
    point_factory_at(&worker, account.trading_account.id(), &candidate).await?;
    let state = account.trading_account.view_state().await?;

    for _ in 0..2 {
        upgrade(&account, &candidate).await?.into_result()?;

        assert_eq!(
            global_code_hash(&worker, account.trading_account.id()).await?,
            candidate
        );
        assert_eq!(account.trading_account.view_state().await?, state);
    }
    assert_upgraded_intact(&account, &agent, &agents).await
}

/// Stands in for intents.near: `mt_transfer` records its args, predecessor, attached deposit and
/// prepaid gas, which `args`, `predecessor`, `deposit` (16 bytes) and `gas` (8 bytes) return raw,
/// little-endian.
fn stub_intents() -> Result<Vec<u8>> {
    Ok(wat::parse_str(
        r#"(module
  (import "env" "input" (func $input (param i64)))
  (import "env" "predecessor_account_id" (func $predecessor (param i64)))
  (import "env" "attached_deposit" (func $attached_deposit (param i64)))
  (import "env" "prepaid_gas" (func $prepaid_gas (result i64)))
  (import "env" "register_len" (func $register_len (param i64) (result i64)))
  (import "env" "read_register" (func $read_register (param i64 i64)))
  (import "env" "storage_write" (func $storage_write (param i64 i64 i64 i64 i64) (result i64)))
  (import "env" "storage_read" (func $storage_read (param i64 i64 i64) (result i64)))
  (import "env" "value_return" (func $value_return (param i64 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "apdg")
  (func $save_register (param $key i64)
    (call $read_register (i64.const 0) (i64.const 1024))
    (drop (call $storage_write (i64.const 1) (local.get $key)
      (call $register_len (i64.const 0)) (i64.const 1024) (i64.const 1))))
  (func $load (param $key i64)
    (drop (call $storage_read (i64.const 1) (local.get $key) (i64.const 0)))
    (call $read_register (i64.const 0) (i64.const 1024))
    (call $value_return (call $register_len (i64.const 0)) (i64.const 1024)))
  (func (export "mt_transfer")
    (call $input (i64.const 0))
    (call $save_register (i64.const 0))
    (call $predecessor (i64.const 0))
    (call $save_register (i64.const 1))
    (call $attached_deposit (i64.const 2048))
    (drop (call $storage_write (i64.const 1) (i64.const 2) (i64.const 16) (i64.const 2048) (i64.const 1)))
    (i64.store (i32.const 2048) (call $prepaid_gas))
    (drop (call $storage_write (i64.const 1) (i64.const 3) (i64.const 8) (i64.const 2048) (i64.const 1))))
  (func (export "args") (call $load (i64.const 0)))
  (func (export "predecessor") (call $load (i64.const 1)))
  (func (export "deposit") (call $load (i64.const 2)))
  (func (export "gas") (call $load (i64.const 3))))"#,
    )?)
}

/// Puts `id` into the sandbox with `wasm` deployed. Patching state reaches account IDs, such as
/// intents.near or the mainnet trading account, that the sandbox's registrar can't create.
async fn patched_contract(worker: &Worker<Sandbox>, id: &str, wasm: &[u8]) -> Result<Contract> {
    let id: AccountId = id.parse()?;
    let sk = SecretKey::from_random(KeyType::ED25519);
    worker
        .patch(&id)
        .account(AccountDetailsPatch::default().balance(NearToken::from_near(10)))
        .access_key(sk.public_key(), AccessKey::full_access())
        .code(wasm)
        .transact()
        .await?;
    Ok(Contract::from_secret_key(id, sk, worker))
}

/// The sandbox accounts the swap tests use.
struct SwapAccounts {
    /// The trading account, at the real quote's recipient.
    trading_account: Contract,
    /// The stub intents.near.
    intents: Contract,
    owner: Account,
    agent: Account,
}

async fn deploy_for_swap(worker: &Worker<Sandbox>) -> Result<SwapAccounts> {
    let intents = patched_contract(worker, "intents.near", &stub_intents()?).await?;
    let trading_account =
        patched_contract(worker, REAL_QUOTE_ACCOUNT, TRADING_ACCOUNT_WASM).await?;
    let owner = worker.dev_create_account().await?;
    trading_account
        .call("new")
        .args_json(json!({ "owner_id": owner.id() }))
        .transact()
        .await?
        .into_result()?;
    let agent = worker.dev_create_account().await?;
    owner
        .call(trading_account.id(), "add_agent")
        .args_json(json!({ "account_id": agent.id() }))
        .transact()
        .await?
        .into_result()?;
    Ok(SwapAccounts {
        trading_account,
        intents,
        owner,
        agent,
    })
}

/// The real quote's recipient and refundTo.
const REAL_QUOTE_ACCOUNT: &str = "implicit_2dde80947980ec0aa6242405.auth.peerfolio.near";

/// The real 1Click quote, `{"payload", "signature"}`.
fn real_quote() -> serde_json::Value {
    serde_json::from_str(include_str!("../res/one_click_quote.json")).unwrap()
}

/// The real quote's terms with a deadline far off and `changes` applied, signed with the test key
/// scripts/test.sh compiles into the build (ONE_CLICK_TEST_KEY), the way 1Click signs.
fn test_quote(changes: serde_json::Value) -> serde_json::Value {
    let mut payload: serde_json::Value =
        serde_json::from_str(real_quote()["payload"].as_str().unwrap()).unwrap();
    payload["deadline"] = json!("2100-01-01T00:00:00Z");
    for (key, value) in changes.as_object().unwrap() {
        payload[key] = value.clone();
    }
    let payload = payload.to_string();
    let message = bs58::encode(crate::utils::hash_payload(payload.as_bytes())).into_string();
    let key = near_crypto::SecretKey::from_seed(near_crypto::KeyType::ED25519, "1click");
    json!({ "payload": payload, "signature": key.sign(message.as_bytes()).to_string() })
}

/// `caller` swaps `quote` on `trading_account`, with 1 yocto and `gas`.
async fn swap(
    caller: &Account,
    trading_account: &Contract,
    quote: &serde_json::Value,
    gas: Gas,
) -> Result<near_workspaces::result::ExecutionFinalResult> {
    Ok(caller
        .call(trading_account.id(), "swap")
        .args_json(json!({
            "quote_payload": quote["payload"],
            "quote_signature": quote["signature"],
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(gas)
        .transact()
        .await?)
}

// swap's cross-contract call reaches intents.near from the trading account itself, with the 1 yocto
// and the transfer taken from the quote, and all the gas swap doesn't use. Whether the real
// intents.near accepts a contract caller is only shown on mainnet.
#[tokio::test]
async fn test_swap_calls_intents_as_the_trading_account() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let SwapAccounts {
        trading_account,
        intents,
        agent,
        ..
    } = deploy_for_swap(&worker).await?;

    let outcome = swap(
        &agent,
        &trading_account,
        &test_quote(json!({})),
        Gas::from_tgas(100),
    )
    .await?;
    assert!(outcome.is_success(), "{:#?}", outcome.failures());
    println!(
        "swap burnt {} Tgas over {} receipts",
        outcome.total_gas_burnt.as_tgas(),
        outcome.receipt_outcomes().len()
    );

    let predecessor = intents.view("predecessor").await?.result;
    assert_eq!(predecessor, trading_account.id().as_bytes());
    let deposit = intents.view("deposit").await?.result;
    assert_eq!(u128::from_le_bytes(deposit.try_into().unwrap()), 1);
    let args: serde_json::Value = intents.view("args").await?.json()?;
    assert_eq!(
        args,
        json!({
            "receiver_id": "101823db08284897878219787860f301b04e4e8e51689387b1c395fb73e99ef5",
            "token_id": "nep141:wrap.near",
            "amount": "20000000000000000000000",
        })
    );
    // 100 Tgas, less swap's own work and the callback's 10, goes to mt_transfer.
    let gas = u64::from_le_bytes(intents.view("gas").await?.result.try_into().unwrap());
    println!("mt_transfer got {} Tgas", Gas::from_gas(gas).as_tgas());
    assert!(
        gas > Gas::from_tgas(80).as_gas(),
        "mt_transfer got {gas} gas"
    );
    Ok(())
}

// GAS_FOR_SWAP is enough for the whole swap, as docs/reference.md promises.
#[tokio::test]
async fn test_swap_succeeds_with_gas_for_swap() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let SwapAccounts {
        trading_account,
        intents,
        agent,
        ..
    } = deploy_for_swap(&worker).await?;

    let gas = crate::GAS_FOR_SWAP;
    let outcome = swap(&agent, &trading_account, &test_quote(json!({})), gas).await?;
    assert!(outcome.is_success(), "{:#?}", outcome.failures());
    let gas = u64::from_le_bytes(intents.view("gas").await?.result.try_into().unwrap());
    assert!(gas >= crate::MT_TRANSFER_MIN_GAS.as_gas());
    Ok(())
}

// The owner can swap too, here liquidating to itself; anyone else is refused.
#[tokio::test]
async fn test_swap_callers() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let SwapAccounts {
        trading_account,
        owner,
        ..
    } = deploy_for_swap(&worker).await?;
    let quote = test_quote(json!({ "recipient": owner.id() }));

    let outcome = swap(&owner, &trading_account, &quote, Gas::from_tgas(100)).await?;
    assert!(outcome.is_success(), "{:#?}", outcome.failures());

    let stranger = worker.dev_create_account().await?;
    let outcome = swap(&stranger, &trading_account, &quote, Gas::from_tgas(100)).await?;
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("Unauthorized: only the owner or an agent can swap"),
        "{}",
        failures
    );
    Ok(())
}

// The deadline is checked against the block time.
#[tokio::test]
async fn test_swap_rejects_an_expired_quote() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let SwapAccounts {
        trading_account,
        agent,
        ..
    } = deploy_for_swap(&worker).await?;
    let quote = test_quote(json!({ "deadline": "2026-01-01T00:00:00Z" }));

    let outcome = swap(&agent, &trading_account, &quote, Gas::from_tgas(100)).await?;
    let failures = format!("{:?}", outcome.failures());
    assert!(
        failures.contains("Quote deadline 2026-01-01T00:00:00Z has passed"),
        "{}",
        failures
    );
    Ok(())
}

// The real 1Click quote verifies against 1Click's key through the real host's ed25519_verify.
// After its deadline (2026-10-25) it's refused as expired, which also comes after the signature
// check, so either outcome shows the signature verified.
#[tokio::test]
async fn test_swap_verifies_a_real_1click_quote() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let SwapAccounts {
        trading_account,
        agent,
        ..
    } = deploy_for_swap(&worker).await?;

    let outcome = swap(&agent, &trading_account, &real_quote(), Gas::from_tgas(100)).await?;
    let failures = format!("{:?}", outcome.failures());
    assert!(
        outcome.is_success()
            || failures.contains("Quote deadline 2026-10-25T16:21:59.865Z has passed"),
        "{}",
        failures
    );
    Ok(())
}
