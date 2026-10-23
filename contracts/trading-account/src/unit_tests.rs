use crate::TradingAccountContract;
use crate::quote::{self, QuoteTerms, SignedQuote};
use near_sdk::json_types::U128;
use near_sdk::mock::{MockAction, Receipt};
use near_sdk::test_utils::{VMContextBuilder, accounts, get_created_receipts};
use near_sdk::{AccountId, Gas, GasWeight, NearToken, Promise, testing_env};
use std::collections::BTreeMap;

const PUBLIC_KEY: &str = "ed25519:11111111111111111111111111111111";

fn owner() -> AccountId {
    accounts(1)
}

/// The agent in these tests.
fn agent() -> AccountId {
    accounts(2)
}

/// Neither the owner nor an agent.
fn stranger() -> AccountId {
    accounts(3)
}

fn user(i: usize) -> AccountId {
    format!("user{}.testnet", i).parse().unwrap()
}

fn get_context(predecessor: AccountId) -> VMContextBuilder {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(predecessor)
        .prepaid_gas(Gas::from_tgas(150));
    builder
}

/// Makes `caller` the predecessor of the next call. Contract state carries over.
fn call_as(caller: AccountId) {
    testing_env!(get_context(caller).build());
}

/// A new trading account owned by `owner()`, who is also the current caller.
fn trading_account() -> TradingAccountContract {
    call_as(owner());
    TradingAccountContract::new(owner())
}

/// A trading account with `agent()` authorized.
fn trading_account_with_agent() -> TradingAccountContract {
    let mut contract = trading_account();
    contract.add_agent(agent());
    contract
}

/// The receipts `promise` schedules. A promise is only scheduled when it's dropped.
fn receipts_of(promise: Promise) -> Vec<Receipt> {
    drop(promise);
    get_created_receipts()
}

#[test]
fn test_new_sets_owner() {
    let contract = trading_account();
    assert_eq!(contract.get_owner_id(), owner());
    assert!(contract.get_agents().is_empty());
}

#[test]
fn test_add_and_remove_agent() {
    let mut contract = trading_account();

    contract.add_agent(agent());
    assert!(contract.is_agent(agent()));

    contract.remove_agent(agent());
    assert!(!contract.is_agent(agent()));
}

#[test]
fn test_is_agent_is_false_for_owner() {
    let contract = trading_account();
    assert!(!contract.is_agent(owner()));
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_add_agent_rejects_non_owner() {
    let mut contract = trading_account();
    call_as(agent());
    contract.add_agent(stranger());
}

#[test]
fn test_get_agents() {
    let mut contract = trading_account();
    contract.add_agent(agent());
    contract.add_agent(stranger());

    let users = contract.get_agents();
    assert_eq!(users.len(), 2);
    assert!(users.contains(&agent()));
    assert!(users.contains(&stranger()));
}

#[test]
#[should_panic(expected = "Maximum number of agents reached:(10)")]
fn test_add_agent_rejects_more_than_max() {
    let mut contract = trading_account();
    for i in 0..10 {
        contract.add_agent(user(i));
    }
    contract.add_agent(user(11));
}

#[test]
fn test_remove_agent_frees_a_slot() {
    let mut contract = trading_account();
    for i in 0..10 {
        contract.add_agent(user(i));
    }
    assert_eq!(contract.get_agents().len(), 10);

    contract.remove_agent(user(0));
    assert_eq!(contract.get_agents().len(), 9);

    contract.add_agent(user(11));
    assert_eq!(contract.get_agents().len(), 10);
    assert!(contract.is_agent(user(11)));
}

#[test]
fn test_add_full_access_key_by_owner() {
    let mut contract = trading_account();
    let receipts = receipts_of(contract.add_full_access_key(PUBLIC_KEY.parse().unwrap()));

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].receiver_id, near_sdk::env::current_account_id());
    assert!(
        matches!(
            &receipts[0].actions[..],
            [MockAction::AddKeyWithFullAccess { public_key, .. }] if public_key.to_string() == PUBLIC_KEY
        ),
        "unexpected actions: {:?}",
        receipts[0].actions
    );
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_add_full_access_key_rejects_non_owner() {
    let mut contract = trading_account_with_agent();
    call_as(agent());
    let _ = contract.add_full_access_key(PUBLIC_KEY.parse().unwrap());
}

#[test]
fn test_add_full_access_key_and_register_with_intents_by_owner() {
    let mut contract = trading_account();
    let mut context = get_context(owner());
    context.attached_deposit(NearToken::from_yoctonear(1));
    testing_env!(context.build());
    let receipts = receipts_of(
        contract.add_full_access_key_and_register_with_intents(PUBLIC_KEY.parse().unwrap()),
    );

    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].receiver_id, near_sdk::env::current_account_id());
    assert!(
        matches!(
            &receipts[0].actions[..],
            [MockAction::AddKeyWithFullAccess { public_key, .. }] if public_key.to_string() == PUBLIC_KEY
        ),
        "unexpected actions: {:?}",
        receipts[0].actions
    );
    // Then intents.near's add_public_key, with the 1 yoctoNEAR it requires.
    assert_eq!(receipts[1].receiver_id.as_str(), "intents.near");
    let [
        MockAction::FunctionCallWeight {
            method_name,
            args,
            attached_deposit,
            ..
        },
    ] = &receipts[1].actions[..]
    else {
        panic!("unexpected actions: {:?}", receipts[1].actions);
    };
    assert_eq!(method_name, b"add_public_key");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(args).unwrap(),
        serde_json::json!({ "public_key": PUBLIC_KEY })
    );
    assert_eq!(*attached_deposit, NearToken::from_yoctonear(1));
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_add_full_access_key_and_register_with_intents_rejects_non_owner() {
    let mut contract = trading_account_with_agent();
    let mut context = get_context(agent());
    context.attached_deposit(NearToken::from_yoctonear(1));
    testing_env!(context.build());
    let _ = contract.add_full_access_key_and_register_with_intents(PUBLIC_KEY.parse().unwrap());
}

/// Calls delete_key as `caller`, attaching `deposit` yoctoNEAR.
fn delete_key_as(
    contract: &mut TradingAccountContract,
    caller: AccountId,
    deposit: u128,
) -> Promise {
    let mut context = get_context(caller);
    context.attached_deposit(NearToken::from_yoctonear(deposit));
    testing_env!(context.build());
    contract.delete_key(PUBLIC_KEY.parse().unwrap())
}

#[test]
fn test_delete_key_by_owner() {
    let mut contract = trading_account();
    let receipts = receipts_of(delete_key_as(&mut contract, owner(), 1));

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].receiver_id, near_sdk::env::current_account_id());
    assert!(
        matches!(
            &receipts[0].actions[..],
            [MockAction::DeleteKey { public_key, .. }] if public_key.to_string() == PUBLIC_KEY
        ),
        "unexpected actions: {:?}",
        receipts[0].actions
    );
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_delete_key_rejects_agent() {
    let mut contract = trading_account_with_agent();
    let _ = delete_key_as(&mut contract, agent(), 1);
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_delete_key_rejects_unauthorized_caller() {
    let mut contract = trading_account();
    let _ = delete_key_as(&mut contract, stranger(), 1);
}

// Exactly 1 yoctoNEAR means the call must be signed with a full-access key, not a function-call key.
#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_delete_key_rejects_no_deposit() {
    let mut contract = trading_account();
    let _ = delete_key_as(&mut contract, owner(), 0);
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_delete_key_rejects_more_than_one_yocto() {
    let mut contract = trading_account();
    let _ = delete_key_as(&mut contract, owner(), 2);
}

// ---- Versioned state and migrate ----

/// A 64-character implicit account. Every v0 trading account is owned by one, so v0 state
/// always starts with the byte 64 (the length of owner_id).
fn implicit_owner() -> AccountId {
    "0123456789abcdef".repeat(4).parse().unwrap()
}

/// v0's state layout, copied from the v0 source: no version byte, and the agents set was
/// named authorized_users.
#[derive(near_sdk::borsh::BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
struct StateV0Fixture {
    owner_id: AccountId,
    authorized_users: near_sdk::collections::UnorderedSet<AccountId>,
    signer_id: AccountId,
}

fn state_bytes(contract: &TradingAccountContract) -> Vec<u8> {
    near_sdk::borsh::to_vec(contract).unwrap()
}

/// Writes v0 state with `agents` agents, as a v0 account stores it. Returns the STATE bytes.
fn write_v0_state(agents: usize) -> Vec<u8> {
    call_as(owner());
    let mut authorized_users = near_sdk::collections::UnorderedSet::new(b"a");
    for i in 0..agents {
        authorized_users.insert(&user(i));
    }
    let raw = near_sdk::borsh::to_vec(&StateV0Fixture {
        owner_id: implicit_owner(),
        authorized_users,
        signer_id: "v1.signer".parse().unwrap(),
    })
    .unwrap();
    near_sdk::env::storage_write(b"STATE", &raw);
    raw
}

/// Writes current-version state with `agents` agents. Returns the STATE bytes.
fn write_current_state(agents: usize) -> Vec<u8> {
    call_as(implicit_owner());
    let mut contract = TradingAccountContract::new(implicit_owner());
    for i in 0..agents {
        contract.add_agent(user(i));
    }
    let raw = state_bytes(&contract);
    near_sdk::env::storage_write(b"STATE", &raw);
    raw
}

fn base64(bytes: &[u8]) -> String {
    near_sdk::base64::Engine::encode(&near_sdk::base64::engine::general_purpose::STANDARD, bytes)
}

/// The one NEP-297 event logged since the last testing_env.
fn logged_event() -> serde_json::Value {
    let logs = near_sdk::test_utils::get_logs();
    let events: Vec<&str> = logs
        .iter()
        .filter_map(|log| log.strip_prefix("EVENT_JSON:"))
        .collect();
    assert_eq!(
        events.len(),
        1,
        "expected exactly one event, logs: {:?}",
        logs
    );
    serde_json::from_str(events[0]).unwrap()
}

fn migrated_event(from: u8, raw: &[u8], agents: Vec<AccountId>) -> serde_json::Value {
    serde_json::json!({
        "standard": "trading_account",
        "version": "1.0.0",
        "event": "migrated",
        "data": [{
            "from_state_version": from,
            "to_state_version": crate::STATE_VERSION,
            "state": base64(raw),
            "agents": agents,
        }],
    })
}

#[test]
fn test_contract_version() {
    assert_eq!(
        serde_json::to_value(trading_account().contract_version()).unwrap(),
        serde_json::json!({ "contract_version": "1.0.0", "state_version": 1 })
    );
}

// A fixed fixture's STATE bytes. If this fails, the state's shape changed: bump STATE_VERSION,
// add a migration from the old shape, and snapshot the new shape as schema/state_v{N}.hex.
#[test]
fn test_state_bytes_match_the_snapshot() {
    assert_eq!(crate::STATE_VERSION, 1, "snapshot the new state version");
    call_as(implicit_owner());
    let mut contract = TradingAccountContract::new(implicit_owner());
    contract.add_agent(user(0));
    contract.add_agent(user(1));
    assert_eq!(
        hex::encode(state_bytes(&contract)),
        include_str!("../schema/state_v1.hex").trim()
    );
}

#[test]
fn test_migrate_v0_keeps_owner_and_agents_and_drops_signer() {
    let raw = write_v0_state(10);
    let mut contract = TradingAccountContract::migrate();

    assert_eq!(contract.get_owner_id(), implicit_owner());
    assert_eq!(contract.get_agents(), (0..10).map(user).collect::<Vec<_>>());
    assert!(contract.is_agent(user(9)));
    // The set is moved, not rebuilt: the new state is the version byte, then v0's bytes up to
    // signer_id (a 4-byte length and the name), which is dropped.
    let signer_len = 4 + "v1.signer".len();
    let migrated = state_bytes(&contract);
    assert_eq!(migrated[0], crate::STATE_VERSION);
    assert_eq!(&migrated[1..], &raw[..raw.len() - signer_len]);
    // The owner still manages the migrated set.
    call_as(implicit_owner());
    contract.remove_agent(user(0));
    assert!(!contract.is_agent(user(0)));
}

#[test]
#[should_panic(expected = "Maximum number of agents reached")]
fn test_migrate_v0_keeps_the_agent_count() {
    write_v0_state(10);
    let mut contract = TradingAccountContract::migrate();
    call_as(implicit_owner());
    contract.add_agent(user(10));
}

#[test]
fn test_migrate_v0_logs_the_pre_migration_state_and_agents() {
    let raw = write_v0_state(3);
    let _ = TradingAccountContract::migrate();
    // v0 has no version byte; the event reports it as state version 0.
    assert_eq!(
        logged_event(),
        migrated_event(0, &raw, (0..3).map(user).collect())
    );
}

#[test]
fn test_migrate_at_current_version_leaves_state_unchanged() {
    let raw = write_current_state(2);
    let contract = TradingAccountContract::migrate();
    assert_eq!(state_bytes(&contract), raw);
    assert_eq!(
        logged_event(),
        migrated_event(crate::STATE_VERSION, &raw, vec![user(0), user(1)])
    );
}

#[test]
#[should_panic(expected = "downgrade not supported")]
fn test_migrate_rejects_a_newer_state_version() {
    let mut raw = write_current_state(0);
    raw[0] = crate::STATE_VERSION + 1;
    near_sdk::env::storage_write(b"STATE", &raw);
    let _ = TradingAccountContract::migrate();
}

#[test]
#[should_panic(expected = "unknown state version")]
fn test_migrate_rejects_state_version_zero() {
    let mut raw = write_current_state(0);
    raw[0] = 0;
    near_sdk::env::storage_write(b"STATE", &raw);
    let _ = TradingAccountContract::migrate();
}

#[test]
#[should_panic(expected = "no state to migrate")]
fn test_migrate_without_state_panics() {
    call_as(owner());
    let _ = TradingAccountContract::migrate();
}

// ---- upgrade and do_upgrade ----

/// The factory's current code hash in these tests.
const LATEST_HASH: &str = "6ziTqYXTX4ASca2dRmgPhVV84jLLLUre4Tym82Lnsf2f";

/// A trading account created by the auth.peerfolio.near factory.
fn factory_created_account_id() -> AccountId {
    "a1b2.auth.peerfolio.near".parse().unwrap()
}

/// Makes `caller` the predecessor of the next call on `factory_created_account_id()`, attaching
/// `deposit` yoctoNEAR.
fn call_on_factory_created_account(caller: AccountId, deposit: u128) {
    let mut context = get_context(caller);
    context
        .current_account_id(factory_created_account_id())
        .attached_deposit(NearToken::from_yoctonear(deposit));
    testing_env!(context.build());
}

fn upgrade_as(caller: AccountId, deposit: u128) -> Promise {
    let mut contract = trading_account_with_agent();
    call_on_factory_created_account(caller, deposit);
    contract.upgrade(LATEST_HASH.to_string())
}

fn do_upgrade(expected_hash: &str, latest: &str) -> Promise {
    let contract = trading_account();
    let account = factory_created_account_id();
    call_on_factory_created_account(account, 0);
    contract.do_upgrade(expected_hash.to_string(), latest.to_string())
}

#[test]
fn test_upgrade_asks_its_factory_then_calls_do_upgrade() {
    let receipts = receipts_of(upgrade_as(owner(), 1));

    assert_eq!(receipts.len(), 2);
    // The factory is the account's parent, never a caller-supplied account.
    assert_eq!(receipts[0].receiver_id.as_str(), "auth.peerfolio.near");
    let [
        MockAction::FunctionCallWeight {
            method_name,
            attached_deposit,
            ..
        },
    ] = &receipts[0].actions[..]
    else {
        panic!("unexpected actions: {:?}", receipts[0].actions);
    };
    assert_eq!(method_name, b"get_proxy_code_base58_hash");
    assert_eq!(*attached_deposit, NearToken::from_yoctonear(0));

    assert_eq!(receipts[1].receiver_id, factory_created_account_id());
    assert_eq!(
        receipts[1].receipt_indices,
        vec![0],
        "runs after the factory view"
    );
    let [
        MockAction::FunctionCallWeight {
            method_name,
            args,
            attached_deposit,
            prepaid_gas,
            ..
        },
    ] = &receipts[1].actions[..]
    else {
        panic!("unexpected actions: {:?}", receipts[1].actions);
    };
    assert_eq!(method_name, b"do_upgrade");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(args).unwrap(),
        serde_json::json!({ "expected_hash": LATEST_HASH })
    );
    assert_eq!(*attached_deposit, NearToken::from_yoctonear(0));
    // do_upgrade must have enough gas left to hand migrate its budget.
    assert!(*prepaid_gas > crate::MIGRATE_GAS, "{:?}", prepaid_gas);
}

// One yoctoNEAR means the owner signed with a full-access key, not a function-call key.
#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_upgrade_requires_one_yocto() {
    let _ = upgrade_as(owner(), 0);
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_upgrade_rejects_an_agent() {
    let _ = upgrade_as(agent(), 1);
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_upgrade_rejects_a_stranger() {
    let _ = upgrade_as(stranger(), 1);
}

// NEAR reverts a receipt only as a whole. Split across receipts (a .then), a failed migrate
// would leave the swapped code running on unmigrated state.
#[test]
fn test_do_upgrade_switches_code_then_migrates_in_one_receipt() {
    let receipts = receipts_of(do_upgrade(LATEST_HASH, LATEST_HASH));

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].receiver_id, factory_created_account_id());
    let [
        use_global_contract,
        MockAction::FunctionCallWeight {
            method_name,
            args,
            attached_deposit,
            prepaid_gas,
            ..
        },
    ] = &receipts[0].actions[..]
    else {
        panic!("unexpected actions: {:?}", receipts[0].actions);
    };
    assert_eq!(
        serde_json::to_value(use_global_contract).unwrap()["UseGlobalContract"]["contract_id"],
        serde_json::json!({ "hash": LATEST_HASH })
    );
    assert_eq!(method_name, b"migrate");
    assert!(args.is_empty());
    assert_eq!(*attached_deposit, NearToken::from_yoctonear(0));
    assert_eq!(*prepaid_gas, crate::MIGRATE_GAS);
}

// The signed expected_hash and the factory's pointer must agree, so moving either one alone
// can't redirect an upgrade.
#[test]
#[should_panic(expected = "factory pointer does not match expected_hash")]
fn test_do_upgrade_rejects_a_hash_the_factory_does_not_point_to() {
    let _ = do_upgrade("11111111111111111111111111111111", LATEST_HASH);
}

#[test]
#[should_panic(expected = "invalid code hash")]
fn test_do_upgrade_rejects_an_invalid_hash() {
    let _ = do_upgrade("not-a-hash", "not-a-hash");
}

// --- Quotes: 1Click's signature and the terms swap accepts ---

/// The trading account in these tests: the mocked chain's current account.
fn trading_account_id() -> AccountId {
    // VMContextBuilder's default, not accounts(0), which is "alice".
    "alice.near".parse().unwrap()
}

/// The accounts a quote may pay out and refund to.
fn ours() -> [AccountId; 2] {
    [trading_account_id(), owner()]
}

const DEPOSIT_ADDRESS: &str = "101823db08284897878219787860f301b04e4e8e51689387b1c395fb73e99ef5";

/// Signs quotes in place of 1Click.
fn test_quote_key() -> near_crypto::SecretKey {
    near_crypto::SecretKey::from_seed(near_crypto::KeyType::ED25519, "1click")
}

fn test_quote_keys() -> [[u8; 32]; 1] {
    [test_quote_key().public_key().key_data().try_into().unwrap()]
}

/// A live quote's signed payload, paying out and refunding to the trading account. `changes`
/// overrides fields; a null removes one. serde_json sorts keys, as 1Click's SDK does.
fn quote_payload(changes: serde_json::Value) -> String {
    let mut payload = serde_json::json!({
        "amount": "20000000000000000000000",
        "amountIn": "20000000000000000000000",
        "amountOut": "98641",
        "deadline": "2026-10-25T16:21:59.865Z",
        "depositAddress": DEPOSIT_ADDRESS,
        "depositType": "INTENTS",
        "destinationAsset": "nep141:17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1",
        "dry": false,
        "minAmountOut": "97654",
        "originAsset": "nep141:wrap.near",
        "recipient": "alice.near",
        "recipientType": "INTENTS",
        "refundTo": "alice.near",
        "refundType": "INTENTS",
        "slippageTolerance": 100,
        "swapType": "EXACT_INPUT",
        "timestamp": "2026-10-22T16:07:00.045Z",
    });
    for (key, value) in changes.as_object().unwrap() {
        if value.is_null() {
            payload.as_object_mut().unwrap().remove(key);
        } else {
            payload[key] = value.clone();
        }
    }
    payload.to_string()
}

/// `payload` signed the way 1Click signs: the base58 text of its SHA-256.
fn sign_quote(payload: &str) -> SignedQuote {
    let message =
        near_sdk::bs58::encode(crate::utils::hash_payload(payload.as_bytes())).into_string();
    SignedQuote {
        payload: payload.to_string(),
        signature: test_quote_key().sign(message.as_bytes()).to_string(),
    }
}

/// The checked terms of a test-signed quote.
fn quote_terms(changes: serde_json::Value) -> QuoteTerms {
    quote::verify(&sign_quote(&quote_payload(changes)), &test_quote_keys()).unwrap()
}

/// The test quote's `deadline` in Unix milliseconds: 2026-10-25T16:21:59.865Z.
const QUOTE_DEADLINE_MS: u64 = 1_792_945_319_865;

/// QuoteTerms::check for a test-signed quote with `changes`, a millisecond before its deadline.
fn check(changes: serde_json::Value) -> Result<(), String> {
    quote_terms(changes).check(&ours(), QUOTE_DEADLINE_MS - 1)
}

/// The real quote's recipient and refundTo.
const REAL_QUOTE_ACCOUNT: &str = "implicit_2dde80947980ec0aa6242405.auth.peerfolio.near";

/// A live quote 1Click returned on 2026-10-22: its signature, and the payload Peerfolio Backend's
/// build_signed_payload flattens it to. It proves the contract hashes and verifies exactly as
/// 1Click's SDK does, with the compiled-in key.
fn real_quote() -> SignedQuote {
    serde_json::from_str(include_str!("../res/one_click_quote.json")).unwrap()
}

#[test]
fn test_manager_key_is_the_one_in_1clicks_sdk() {
    let sdk_key = near_sdk::bs58::decode("reYaWhvwu8Jzo3WUM3zhn6VrhuMEF4eADL17qtRVifc")
        .into_vec()
        .unwrap();
    assert_eq!(quote::ONE_CLICK_MANAGER_KEYS, [sdk_key.as_slice()]);
}

// Without ONE_CLICK_TEST_KEY at build time, as in every release build, only 1Click's key is
// trusted.
#[test]
fn test_trusted_keys_are_1clicks_alone() {
    assert_eq!(
        option_env!("ONE_CLICK_TEST_KEY"),
        None,
        "unit tests run without a test key"
    );
    assert_eq!(quote::trusted_keys(), quote::ONE_CLICK_MANAGER_KEYS);
}

#[test]
fn test_verify_accepts_a_real_1click_quote() {
    let terms = quote::verify(&real_quote(), quote::ONE_CLICK_MANAGER_KEYS).unwrap();
    assert_eq!(terms.recipient, REAL_QUOTE_ACCOUNT);
    assert_eq!(terms.deposit_address.as_deref(), Some(DEPOSIT_ADDRESS));
}

#[test]
fn test_verify_rejects_a_tampered_real_quote() {
    let mut quote = real_quote();
    quote.payload = quote.payload.replace(REAL_QUOTE_ACCOUNT, "attacker.near");
    let error = quote::verify(&quote, quote::ONE_CLICK_MANAGER_KEYS).unwrap_err();
    assert!(error.contains("not signed by 1Click"), "{}", error);
}

#[test]
fn test_verify_rejects_a_malformed_signature() {
    for signature in [
        real_quote().signature.trim_start_matches("ed25519:"),
        "secp256k1:2EgRQHTMiMv7UR6z6Tt89CENpuEfD9DpsAKPUqu5aKKSQt4MXFDL5GRDFrRSmk9QLCvQFrxKNLedvKShHBzE2VJn",
        "ed25519:0OIl",
        "ed25519:11111111",
    ] {
        let quote = SignedQuote {
            payload: real_quote().payload,
            signature: signature.to_string(),
        };
        let error = quote::verify(&quote, quote::ONE_CLICK_MANAGER_KEYS).unwrap_err();
        assert!(
            error.contains("Invalid quote signature: expected"),
            "{}",
            error
        );
    }
}

// A signed payload with two `recipient` keys must not parse, whichever one a parser would keep.
#[test]
fn test_verify_rejects_duplicate_keys() {
    let payload = quote_payload(serde_json::json!({})).replacen(
        r#""recipient":"alice.near""#,
        r#""recipient":"alice.near","recipient":"attacker.near""#,
        1,
    );
    let error = quote::verify(&sign_quote(&payload), &test_quote_keys()).unwrap_err();
    assert!(error.contains("duplicate field `recipient`"), "{}", error);
}

#[test]
fn test_check_accepts_a_quote_paying_out_to_the_trading_account() {
    check(serde_json::json!({})).unwrap();
}

// A liquidation pays out to the owner while refunds come back to the trading account.
#[test]
fn test_check_accepts_the_owner_as_recipient_and_refund() {
    for field in ["recipient", "refundTo"] {
        check(serde_json::json!({ field: owner().as_str() })).unwrap();
    }
}

#[test]
fn test_check_accepts_the_maximum_slippage() {
    check(serde_json::json!({ "slippageTolerance": quote::MAX_SLIPPAGE_BPS })).unwrap();
}

// Fields 1Click adds later are ignored.
#[test]
fn test_check_ignores_unknown_fields() {
    check(serde_json::json!({ "someNewField": "value" })).unwrap();
}

// Each quote term that would send funds, or their refund, anywhere but back to the user.
#[test]
fn test_check_rejects_quotes_that_pay_out_elsewhere() {
    let cases = [
        (serde_json::json!({"dry": true}), "dry quote"),
        (
            serde_json::json!({"recipient": "attacker.near"}),
            "Quote recipient attacker.near",
        ),
        (
            serde_json::json!({"refundTo": "attacker.near"}),
            "Quote refundTo attacker.near",
        ),
        (
            serde_json::json!({"recipientType": "DESTINATION_CHAIN"}),
            "recipientType must be INTENTS",
        ),
        (
            serde_json::json!({"refundType": "ORIGIN_CHAIN"}),
            "refundType must be INTENTS",
        ),
        (
            serde_json::json!({"depositType": "ORIGIN_CHAIN"}),
            "depositType must be INTENTS",
        ),
        (
            serde_json::json!({"virtualChainRecipient": "0xattacker"}),
            "virtualChainRecipient is not allowed",
        ),
        (
            serde_json::json!({"virtualChainRefundRecipient": "0xattacker"}),
            "virtualChainRefundRecipient is not allowed",
        ),
        (
            serde_json::json!({"customRecipientMsg": "{}"}),
            "customRecipientMsg is not allowed",
        ),
        (
            serde_json::json!({"slippageTolerance": quote::MAX_SLIPPAGE_BPS + 1}),
            "above the 200 bps maximum",
        ),
    ];
    for (changes, expected) in cases {
        let error = check(changes).unwrap_err();
        assert!(error.contains(expected), "{}: {}", expected, error);
    }
}

// The deadline must be after the block time: a millisecond before it passes, at it fails.
#[test]
fn test_check_rejects_a_quote_at_or_after_its_deadline() {
    let terms = quote_terms(serde_json::json!({}));
    terms.check(&ours(), QUOTE_DEADLINE_MS - 1).unwrap();
    for now_ms in [QUOTE_DEADLINE_MS, QUOTE_DEADLINE_MS + 1, u64::MAX] {
        assert_eq!(
            terms.check(&ours(), now_ms),
            Err("Quote deadline 2026-10-25T16:21:59.865Z has passed".to_string()),
            "now {}",
            now_ms
        );
    }
}

// quote_payload removes a field set to null.
#[test]
fn test_check_rejects_a_missing_deadline() {
    assert_eq!(
        check(serde_json::json!({ "deadline": null })),
        Err("Quote has no deadline".to_string())
    );
}

#[test]
fn test_check_rejects_a_malformed_deadline() {
    assert_eq!(
        check(serde_json::json!({ "deadline": "tomorrow" })),
        Err("Quote deadline tomorrow is not an ISO 8601 date-time".to_string())
    );
}

// Each form names 2026-10-26T10:38:03.537Z, or that minute or second. Checked against Python's
// datetime.
#[test]
fn test_parse_deadline_ms_reads_iso_8601_forms() {
    let cases = [
        ("2026-10-26T10:38:03.537Z", 1_793_011_083_537),
        ("2026-10-26T12:38:03.537+02:00", 1_793_011_083_537),
        ("2026-10-26T12:38:03.537+0200", 1_793_011_083_537),
        ("2026-10-26T10:38:03.537123456Z", 1_793_011_083_537),
        ("2026-10-26 10:38:03.537Z", 1_793_011_083_537),
        ("2026-10-26t10:38:03.537z", 1_793_011_083_537),
        ("2026-10-26T10:38:03Z", 1_793_011_083_000),
        ("2026-10-26T10:38:03.5Z", 1_793_011_083_500),
        ("2026-10-26T10:38Z", 1_793_011_080_000),
        ("20261026T103803Z", 1_793_011_083_000),
        ("2026-W44-1T10:38:03Z", 1_793_011_083_000),
        ("2026-299T10:38:03Z", 1_793_011_083_000),
        // No time zone: read as UTC.
        ("2026-10-26T10:38:03.537", 1_793_011_083_537),
        ("2026-10-26T10:38", 1_793_011_080_000),
    ];
    for (deadline, expected) in cases {
        assert_eq!(
            quote::parse_deadline_ms(deadline),
            Ok(expected),
            "{}",
            deadline
        );
    }
}

#[test]
fn test_parse_deadline_ms_rejects_anything_else() {
    for deadline in [
        "2026-10-26",
        // A zone the parsers reject must not be read as UTC. `time` takes offsets up to 25:59.
        "2026-10-26T10:38:03+26:00",
        "2026-13-26T10:38:03Z",
        "2027-02-29T10:38:03Z",
        "2026-10-26T10:38:03Zjunk",
        "",
        "1793011083537",
    ] {
        assert_eq!(
            quote::parse_deadline_ms(deadline),
            Err(format!(
                "Quote deadline {} is not an ISO 8601 date-time",
                deadline
            )),
            "{:?}",
            deadline
        );
    }
}

// --- swap: depositing into a 1Click quote with a direct call ---

/// The real quote's `timestamp`, when 1Click issued it, in nanoseconds: 2026-10-22T16:07:00.045Z.
const REAL_QUOTE_ISSUED_NS: u64 = 1_792_685_220_045_000_000;

/// A context on the trading account at the real quote's recipient, at the time 1Click issued the
/// quote, called by `caller` with `deposit` attached and `gas` prepaid.
fn swap_context(caller: AccountId, deposit: u128, gas: Gas) -> VMContextBuilder {
    let mut context = get_context(caller);
    context
        .current_account_id(REAL_QUOTE_ACCOUNT.parse().unwrap())
        .block_timestamp(REAL_QUOTE_ISSUED_NS)
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .prepaid_gas(gas);
    context
}

/// The trading account at the real quote's recipient, with `agent()` authorized. The next call
/// is made by `caller` with `deposit` attached.
fn swap_account(caller: AccountId, deposit: u128) -> TradingAccountContract {
    swap_account_with(swap_context(caller, deposit, Gas::from_tgas(150)))
}

fn swap_account_with(context: VMContextBuilder) -> TradingAccountContract {
    testing_env!(swap_context(owner(), 0, Gas::from_tgas(150)).build());
    let mut contract = TradingAccountContract::new(owner());
    contract.add_agent(agent());
    testing_env!(context.build());
    contract
}

fn swap_with(contract: &mut TradingAccountContract, quote: SignedQuote) -> Promise {
    contract.swap(quote.payload, quote.signature)
}

// The transfer comes from the quote alone, and intents.near gets the 1 yocto. mt_transfer gets
// at least its minimum plus all of swap's unused gas; the callback gets a fixed amount.
#[test]
fn test_swap_transfers_the_quoted_deposit() {
    for caller in [agent(), owner()] {
        let mut contract = swap_account(caller.clone(), 1);
        let receipts = receipts_of(swap_with(&mut contract, real_quote()));

        assert_eq!(receipts.len(), 2, "{}", caller);
        assert_eq!(receipts[0].receiver_id.as_str(), "intents.near");
        let [
            MockAction::FunctionCallWeight {
                method_name,
                args,
                attached_deposit,
                prepaid_gas,
                gas_weight,
                ..
            },
        ] = &receipts[0].actions[..]
        else {
            panic!("unexpected actions: {:?}", receipts[0].actions);
        };
        assert_eq!(method_name, b"mt_transfer");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(args).unwrap(),
            serde_json::json!({
                "receiver_id": DEPOSIT_ADDRESS,
                "token_id": "nep141:wrap.near",
                "amount": "20000000000000000000000",
            })
        );
        assert_eq!(*attached_deposit, NearToken::from_yoctonear(1));
        assert_eq!(*prepaid_gas, crate::MT_TRANSFER_MIN_GAS);
        assert_eq!(gas_weight.0, GasWeight(1).0);

        assert_eq!(receipts[1].receiver_id.as_str(), REAL_QUOTE_ACCOUNT);
        let [
            MockAction::FunctionCallWeight {
                method_name,
                prepaid_gas,
                gas_weight,
                ..
            },
        ] = &receipts[1].actions[..]
        else {
            panic!("unexpected actions: {:?}", receipts[1].actions);
        };
        assert_eq!(method_name, b"swap_callback");
        assert_eq!(*prepaid_gas, crate::CALLBACK_GAS);
        assert_eq!(gas_weight.0, 0);
    }
}

/// The message `f` panics with.
fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let payload = std::panic::catch_unwind(f).unwrap_err();
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap()
}

/// The panic message of a swap of the real quote in `context`.
fn swap_panic_with(context: VMContextBuilder) -> String {
    panic_message(|| {
        let mut contract = swap_account_with(context);
        let _ = swap_with(&mut contract, real_quote());
    })
}

/// The panic message of a swap by `caller` with `deposit` attached.
fn swap_panic(caller: AccountId, deposit: u128) -> String {
    swap_panic_with(swap_context(caller, deposit, Gas::from_tgas(150)))
}

#[test]
fn test_swap_rejects_a_stranger() {
    let message = swap_panic(stranger(), 1);
    assert!(
        message.contains("Unauthorized: only the owner or an agent can swap"),
        "{}",
        message
    );
}

#[test]
fn test_swap_requires_one_yocto() {
    for deposit in [0, 2] {
        let message = swap_panic(agent(), deposit);
        assert!(
            message.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
            "deposit {}: {}",
            deposit,
            message
        );
    }
}

// A gas unit below GAS_FOR_SWAP fails. The sandbox test runs a swap at exactly GAS_FOR_SWAP: the
// mocked chain charges more for swap's own work than the real one.
#[test]
fn test_swap_requires_gas_for_swap() {
    let message = swap_panic_with(swap_context(
        agent(),
        1,
        crate::GAS_FOR_SWAP.saturating_sub(Gas::from_gas(1)),
    ));
    assert!(
        message.contains("Not enough gas attached. Please attach at least 30 TGas"),
        "{}",
        message
    );
}

#[test]
#[should_panic(expected = "Invalid quote signature: not signed by 1Click")]
fn test_swap_rejects_a_tampered_quote() {
    let mut contract = swap_account(agent(), 1);
    let mut quote = real_quote();
    quote.payload = quote
        .payload
        .replace("20000000000000000000000", "30000000000000000000000");
    let _ = swap_with(&mut contract, quote);
}

// swap verifies against the compiled-in key, so a test-signed quote is refused.
#[test]
#[should_panic(expected = "Invalid quote signature: not signed by 1Click")]
fn test_swap_rejects_a_quote_not_signed_by_1click() {
    let mut contract = swap_account(agent(), 1);
    let quote =
        sign_quote(&quote_payload(serde_json::json!({})).replace("alice.near", REAL_QUOTE_ACCOUNT));
    let _ = swap_with(&mut contract, quote);
}

// The same genuine quote, on an account it doesn't pay out to.
#[test]
#[should_panic(expected = "is not the trading account or its owner")]
fn test_swap_rejects_a_quote_paying_another_account() {
    let mut contract = trading_account_with_agent();
    testing_env!(
        get_context(agent())
            .block_timestamp(REAL_QUOTE_ISSUED_NS)
            .attached_deposit(NearToken::from_yoctonear(1))
            .build()
    );
    let _ = swap_with(&mut contract, real_quote());
}

// The real quote's deadline is the block time it stops working at.
#[test]
fn test_swap_rejects_an_expired_quote() {
    let mut context = swap_context(agent(), 1, Gas::from_tgas(150));
    context.block_timestamp(QUOTE_DEADLINE_MS * 1_000_000);
    let message = swap_panic_with(context);
    assert!(
        message.contains("Quote deadline 2026-10-25T16:21:59.865Z has passed"),
        "{}",
        message
    );
}

// Quote terms swap rejects (dry, foreign refundTo, slippage over the cap, ...) are covered by
// test_check_rejects_quotes_that_pay_out_elsewhere: swap runs QuoteTerms::check.

#[test]
#[should_panic(expected = "swap: mt_transfer on intents.near failed")]
fn test_swap_callback_fails_when_the_transfer_failed() {
    let contract = trading_account_with_agent();
    testing_env!(
        get_context(accounts(0)).build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![near_sdk::PromiseResult::Failed],
    );
    contract.swap_callback();
}

// --- transfer_to_owner: moving tokens to the owner inside intents ---

/// `n` distinct tokens, each with a non-zero amount.
fn tokens(n: usize) -> BTreeMap<String, U128> {
    (0..n)
        .map(|i| (format!("nep141:token{}.near", i), U128(i as u128 + 1)))
        .collect()
}

/// A trading account with `agent()` authorized. The next call is made by `caller` with
/// `deposit` attached and `gas` prepaid.
fn transfer_account(caller: AccountId, deposit: u128, gas: Gas) -> TradingAccountContract {
    let contract = trading_account_with_agent();
    let mut context = get_context(caller);
    context
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .prepaid_gas(gas);
    testing_env!(context.build());
    contract
}

/// The panic message of a transfer of `tokens` by `caller` with `deposit` attached and `gas`
/// prepaid.
fn transfer_panic(
    caller: AccountId,
    deposit: u128,
    gas: Gas,
    tokens: BTreeMap<String, U128>,
) -> String {
    panic_message(|| {
        let mut contract = transfer_account(caller, deposit, gas);
        let _ = contract.transfer_to_owner(tokens, None);
    })
}

// The batch goes to the owner whatever the caller, in the map's sorted order, with the memo and
// the 1 yocto. mt_batch_transfer gets at least its minimum for the token count plus all the
// unused gas; the callback gets a fixed amount.
#[test]
fn test_transfer_to_owner_sends_one_batch_to_the_owner() {
    for (caller, memo) in [(agent(), Some("rebalance".to_string())), (owner(), None)] {
        let mut contract = transfer_account(caller.clone(), 1, Gas::from_tgas(150));
        let tokens = BTreeMap::from([
            ("nep141:wrap.near".to_string(), U128(5)),
            ("nep141:usdt.tether-token.near".to_string(), U128(7)),
            ("nep245:v2_1.omni.hot.tg:1100_111".to_string(), U128(9)),
        ]);
        let receipts = receipts_of(contract.transfer_to_owner(tokens, memo.clone()));

        assert_eq!(receipts.len(), 2, "{}", caller);
        assert_eq!(receipts[0].receiver_id.as_str(), "intents.near");
        let [
            MockAction::FunctionCallWeight {
                method_name,
                args,
                attached_deposit,
                prepaid_gas,
                gas_weight,
                ..
            },
        ] = &receipts[0].actions[..]
        else {
            panic!("unexpected actions: {:?}", receipts[0].actions);
        };
        assert_eq!(method_name, b"mt_batch_transfer");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(args).unwrap(),
            serde_json::json!({
                "receiver_id": owner(),
                "token_ids": [
                    "nep141:usdt.tether-token.near",
                    "nep141:wrap.near",
                    "nep245:v2_1.omni.hot.tg:1100_111",
                ],
                "amounts": ["7", "5", "9"],
                "memo": memo,
            })
        );
        assert_eq!(*attached_deposit, NearToken::from_yoctonear(1));
        assert_eq!(*prepaid_gas, crate::mt_batch_transfer_min_gas(3));
        assert_eq!(gas_weight.0, GasWeight(1).0);

        assert_eq!(receipts[1].receiver_id, near_sdk::env::current_account_id());
        let [
            MockAction::FunctionCallWeight {
                method_name,
                prepaid_gas,
                gas_weight,
                ..
            },
        ] = &receipts[1].actions[..]
        else {
            panic!("unexpected actions: {:?}", receipts[1].actions);
        };
        assert_eq!(method_name, b"transfer_to_owner_callback");
        assert_eq!(*prepaid_gas, crate::CALLBACK_GAS);
        assert_eq!(gas_weight.0, 0);
    }
}

#[test]
fn test_transfer_to_owner_rejects_a_stranger() {
    let message = transfer_panic(stranger(), 1, Gas::from_tgas(150), tokens(1));
    assert!(
        message.contains("Unauthorized: only the owner or an agent can transfer to the owner"),
        "{}",
        message
    );
}

#[test]
fn test_transfer_to_owner_requires_one_yocto() {
    for deposit in [0, 2] {
        let message = transfer_panic(agent(), deposit, Gas::from_tgas(150), tokens(1));
        assert!(
            message.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
            "deposit {}: {}",
            deposit,
            message
        );
    }
}

// At most 10 tokens, the most the gas was measured for, and an empty batch moves nothing.
#[test]
fn test_transfer_to_owner_rejects_0_or_more_than_10_tokens() {
    for n in [0, 11] {
        let message = transfer_panic(agent(), 1, Gas::from_tgas(150), tokens(n));
        assert!(
            message.contains("tokens must have 1 to 10 entries"),
            "{} tokens: {}",
            n,
            message
        );
    }
}

#[test]
fn test_transfer_to_owner_rejects_a_zero_amount() {
    let mut tokens = tokens(2);
    tokens.insert("nep141:wrap.near".to_string(), U128(0));
    let message = transfer_panic(agent(), 1, Gas::from_tgas(150), tokens);
    assert!(
        message.contains("Amount of nep141:wrap.near must be above 0"),
        "{}",
        message
    );
}

// The gas needed grows with the token count: 38 Tgas for 10 tokens.
#[test]
fn test_transfer_to_owner_requires_its_gas() {
    let needed = crate::gas_for_transfer_to_owner(10);
    let message = transfer_panic(
        agent(),
        1,
        needed.saturating_sub(Gas::from_gas(1)),
        tokens(10),
    );
    assert!(
        message.contains("Not enough gas attached. Please attach at least 38 TGas"),
        "{}",
        message
    );
}

#[test]
#[should_panic(expected = "transfer_to_owner: mt_batch_transfer on intents.near failed")]
fn test_transfer_to_owner_callback_fails_when_the_transfer_failed() {
    let contract = trading_account_with_agent();
    testing_env!(
        get_context(accounts(0)).build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![near_sdk::PromiseResult::Failed],
    );
    contract.transfer_to_owner_callback();
}
