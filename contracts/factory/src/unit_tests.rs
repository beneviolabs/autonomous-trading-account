#![cfg(test)]

use crate::TradingAccountFactory;
use near_sdk::mock::{MockAction, Receipt};
use near_sdk::serde_json::{self, json};
use near_sdk::test_utils::{accounts, get_created_receipts, VMContextBuilder};
use near_sdk::{testing_env, AccountId, Gas, NearToken, Promise, PromiseError};

const ALICE: &str = "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a970f6648cdf0dbd6de";
const VICTIM: &str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
const ATTACKER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CODE_HASH: &str = "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz";
const MIN_DEPOSIT: u128 = 1_000_000; // The factory's minimum deposit, in yoctoNEAR.
const PUBLIC_KEY: &str = "ed25519:11111111111111111111111111111111";

fn id(account_id: &str) -> AccountId {
    account_id.parse().unwrap()
}

fn factory_owner() -> AccountId {
    accounts(1)
}

/// Makes `predecessor` the caller of the next factory call, attaching `deposit` yoctoNEAR.
/// Contract state carries over.
fn call_as(predecessor: &AccountId, deposit: u128) {
    testing_env!(VMContextBuilder::new()
        .predecessor_account_id(predecessor.clone())
        .current_account_id(id("factory.testnet"))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .prepaid_gas(Gas::from_tgas(150))
        .build());
}

/// A testnet factory at factory.testnet, owned by `factory_owner()`.
fn factory() -> TradingAccountFactory {
    call_as(&factory_owner(), MIN_DEPOSIT);
    TradingAccountFactory::new(
        factory_owner(),
        "testnet".to_string(),
        CODE_HASH.to_string(),
    )
}

fn base_name(contract: &TradingAccountFactory, owner_id: &str) -> String {
    contract.get_base_account_name(&id(owner_id))
}

/// The receipts `promise` schedules. A promise is only scheduled when it's dropped.
fn receipts_of(promise: Promise) -> Vec<Receipt> {
    drop(promise);
    get_created_receipts()
}

#[test]
fn test_new_stores_owner_and_code_hash() {
    let contract = factory();
    assert_eq!(contract.get_owner_id(), factory_owner());
    assert_eq!(contract.get_proxy_code_base58_hash(), CODE_HASH);
    assert_eq!(
        contract.get_proxy_code_hash_hex(),
        "c9acef2aaaab73d07684b79b4655f7ab946c7e15e3091f1fae6bc5df86566bd9"
    );
}

#[test]
fn test_new_selects_signer_by_network() {
    call_as(&factory_owner(), 0);
    let contract = TradingAccountFactory::new(
        factory_owner(),
        "mainnet".to_string(),
        CODE_HASH.to_string(),
    );
    assert_eq!(contract.get_signer_contract().as_str(), "v1.signer");
    assert_eq!(
        factory().get_signer_contract().as_str(),
        "v1.signer-prod.testnet"
    );
}

// Regression test for pen test finding #13a: unknown networks used to fall back to testnet.
#[test]
fn test_new_rejects_unknown_network() {
    for network in ["mainner", "MAINNET", "prod", ""] {
        let result = std::panic::catch_unwind(|| {
            call_as(&factory_owner(), 0);
            TradingAccountFactory::new(factory_owner(), network.to_string(), CODE_HASH.to_string())
        });
        assert!(result.is_err(), "network {:?} should be rejected", network);
    }
}

#[test]
fn test_create_proxy_global_by_owner() {
    let mut contract = factory();
    call_as(&id(ALICE), 2_000_000);
    let receipts = receipts_of(contract.create_proxy_global(id(ALICE)));

    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts[0].receiver_id.as_str(),
        "implicit_c9a8418ddb0c0ef15e2c857b.factory.testnet"
    );
    let [MockAction::CreateAccount { .. }, MockAction::Transfer { deposit, .. }, MockAction::UseGlobalContract { contract_id, .. }, MockAction::FunctionCallWeight {
        method_name, args, ..
    }] = &receipts[0].actions[..]
    else {
        panic!("unexpected actions: {:?}", receipts[0].actions);
    };
    // The whole deposit becomes the trading account's balance.
    assert_eq!(*deposit, NearToken::from_yoctonear(2_000_000));
    assert_eq!(
        format!("{:?}", contract_id),
        format!("CodeHash({})", CODE_HASH)
    );
    // The trading account's new(owner_id, signer_id).
    assert_eq!(method_name, b"new");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(args).unwrap(),
        json!({ "owner_id": ALICE, "signer_id": "v1.signer-prod.testnet" })
    );
}

#[test]
fn test_create_proxy_global_binds_owner_to_derived_name() {
    let mut contract = factory();
    call_as(&id(VICTIM), MIN_DEPOSIT);
    // The same argument drives both the name and the owner.
    let _ = contract.create_proxy_global(id(VICTIM));
    let logs = near_sdk::test_utils::get_logs();
    let expected_account = format!("{}.factory.testnet", base_name(&contract, VICTIM));
    assert!(logs[0].contains(&format!("Account: {}, Owner: {}", expected_account, VICTIM)));
}

#[test]
#[should_panic(expected = "owner_id must be a NEAR implicit account")]
fn test_create_proxy_global_rejects_named_owner() {
    let mut contract = factory();
    call_as(&id("alice.near"), MIN_DEPOSIT);
    let _ = contract.create_proxy_global(id("alice.near"));
}

#[test]
#[should_panic(expected = "Only owner_id can create its trading account")]
fn test_create_proxy_global_rejects_non_owner_predecessor() {
    let mut contract = factory();
    call_as(&id(ATTACKER), MIN_DEPOSIT);
    let _ = contract.create_proxy_global(id(VICTIM));
}

#[test]
fn test_deposit_and_create_proxy_global_by_owner() {
    let mut contract = factory();
    call_as(&id(ALICE), MIN_DEPOSIT);
    let receipts = receipts_of(contract.deposit_and_create_proxy_global(id(ALICE)));

    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[0].receiver_id.as_str(),
        "implicit_c9a8418ddb0c0ef15e2c857b.factory.testnet"
    );
    // Then on_proxy_created, which refunds the caller if the creation failed.
    assert_eq!(receipts[1].receiver_id.as_str(), "factory.testnet");
    let [MockAction::FunctionCallWeight {
        method_name, args, ..
    }] = &receipts[1].actions[..]
    else {
        panic!("unexpected actions: {:?}", receipts[1].actions);
    };
    assert_eq!(method_name, b"on_proxy_created");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(args).unwrap(),
        json!({ "original_caller": ALICE, "deposit": MIN_DEPOSIT.to_string() })
    );
}

#[test]
#[should_panic(expected = "Must attach at least 1000 yⓃ")]
fn test_deposit_and_create_proxy_global_rejects_small_deposit() {
    let mut contract = factory();
    call_as(&id(ALICE), MIN_DEPOSIT / 2);
    let _ = contract.deposit_and_create_proxy_global(id(ALICE));
}

#[test]
#[should_panic(expected = "Only owner_id can create its trading account")]
fn test_deposit_and_create_proxy_global_rejects_non_owner_predecessor() {
    let mut contract = factory();
    call_as(&id(ATTACKER), MIN_DEPOSIT);
    let _ = contract.deposit_and_create_proxy_global(id(VICTIM));
}

#[test]
fn test_on_proxy_created_refunds_on_failure() {
    let mut contract = factory();
    let receipts = receipts_of(contract.on_proxy_created(
        id(ALICE),
        Err(PromiseError::Failed),
        NearToken::from_yoctonear(2_000_000),
    ));

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].receiver_id.as_str(), ALICE);
    assert!(
        matches!(
            receipts[0].actions[..],
            [MockAction::Transfer { deposit, .. }] if deposit == NearToken::from_yoctonear(2_000_000)
        ),
        "unexpected actions: {:?}",
        receipts[0].actions
    );
}

#[test]
fn test_get_base_account_name_implicit_format_unchanged() {
    let contract = factory();
    // Pinned to the pre-fix derivation so existing implicit users keep their names.
    assert_eq!(
        base_name(&contract, ALICE),
        "implicit_c9a8418ddb0c0ef15e2c857b"
    );
    let other = base_name(&contract, VICTIM);
    assert_ne!(other, base_name(&contract, ALICE));
    assert!(other.starts_with("implicit_"));
    assert_eq!(other.len(), 33);
}

// Regression tests for pen test finding #1: named ids used to collide with each other
// and with implicit users' names. They are now rejected outright.

#[test]
fn test_get_base_account_name_rejects_non_implicit_ids() {
    let contract = factory();
    for owner_id in [
        "alice.near",
        "alice.testnet",
        "sub.alice.near",
        "sub-alice.near",
        // The pre-fix squat of ALICE's name.
        "implicit_c9a8418ddb0c0ef15e2c857b.near",
        // ETH implicit.
        "0x06012c8cf97bead5deae237070f9587f8e7a266d",
        // 64 chars but not hex: a valid top-level named account anyone can register.
        "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a970f6648cdf0dbd6dg",
        "invalid_account_format",
    ] {
        // Parse outside catch_unwind so only the contract's own check can pass the test.
        let owner: AccountId = owner_id.parse().unwrap();
        let result = std::panic::catch_unwind(|| contract.get_base_account_name(&owner));
        assert!(result.is_err(), "{} should be rejected", owner_id);
    }
}

#[test]
fn test_uppercase_hex_never_reaches_the_contract() {
    // AccountId rejects uppercase, so JSON args carrying one fail before the method runs.
    let uppercase = "98793CD91A3F870FB126F66285808C7E094AFCFC4EDA8A970F6648CDF0DBD6DE";
    assert!(uppercase.parse::<AccountId>().is_err());
}

#[test]
fn test_verify_implicit_base_name() {
    let contract = factory();
    assert!(contract.verify_implicit_base_name(id(ALICE), base_name(&contract, ALICE)));
    assert!(!contract.verify_implicit_base_name(id(ALICE), base_name(&contract, VICTIM)));
}

#[test]
fn test_set_global_code_hash() {
    let mut contract = factory();
    contract.set_global_code_hash("CxhHoMAytiy39MSyKCJRksiWXvhYdRYncFpcVWAd4Pbg".to_string());
    assert_eq!(
        contract.get_proxy_code_base58_hash(),
        "CxhHoMAytiy39MSyKCJRksiWXvhYdRYncFpcVWAd4Pbg"
    );
}

#[test]
#[should_panic(expected = "Only the contract owner can perform this action.")]
fn test_set_global_code_hash_rejects_non_owner() {
    let mut contract = factory();
    call_as(&accounts(3), 0);
    contract.set_global_code_hash("CxhHoMAytiy39MSyKCJRksiWXvhYdRYncFpcVWAd4Pbg".to_string());
}

#[test]
fn test_add_full_access_key_by_owner() {
    let mut contract = factory();
    let receipts = receipts_of(contract.add_full_access_key(PUBLIC_KEY.parse().unwrap()));

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].receiver_id.as_str(), "factory.testnet");
    assert!(
        matches!(
            &receipts[0].actions[..],
            [MockAction::AddKeyWithFullAccess { public_key, .. }] if public_key.to_string() == PUBLIC_KEY
        ),
        "unexpected actions: {:?}",
        receipts[0].actions
    );
}
