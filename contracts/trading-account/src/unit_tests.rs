use crate::quote::{self, QuoteTerms};
use crate::{ActionString, SignedQuote, TradingAccountContract, test_support};
use near_sdk::json_types::{Base58CryptoHash, U64};
use near_sdk::mock::{MockAction, Receipt};
use near_sdk::test_utils::{VMContextBuilder, accounts, get_created_receipts};
use near_sdk::{AccountId, Gas, NearToken, Promise, testing_env};
use omni_transaction::TxBuilder;
use omni_transaction::near::NearTransaction;
use omni_transaction::near::types::Action as OmniAction;
use omni_transaction::near::utils::PublicKeyStrExt;

const PUBLIC_KEY: &str = "ed25519:11111111111111111111111111111111";

/// One `mt_transfer` on `intents.near`, the only call the allowlist permits, to `alice.near`: the
/// mocked chain's current account, so the trading account itself.
const MT_TRANSFER: &str = r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#;

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

/// The block height the mocked chain is at.
const BLOCK_HEIGHT: u64 = 100;

fn get_context(predecessor: AccountId) -> VMContextBuilder {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(predecessor)
        .prepaid_gas(Gas::from_tgas(150))
        .block_height(BLOCK_HEIGHT);
    builder
}

/// Makes `caller` the predecessor of the next call. Contract state carries over.
fn call_as(caller: AccountId) {
    testing_env!(get_context(caller).build());
}

/// A new trading account owned by `owner()`, who is also the current caller.
fn trading_account() -> TradingAccountContract {
    call_as(owner());
    TradingAccountContract::new(owner(), "v1.signer".parse().unwrap())
}

/// A trading account with `agent()` authorized.
fn trading_account_with_agent() -> TradingAccountContract {
    let mut contract = trading_account();
    contract.add_agent(agent());
    contract
}

/// Calls request_signature as `caller`. The tests only vary the receiver and the actions.
fn request_signature(
    contract: &mut TradingAccountContract,
    caller: AccountId,
    contract_id: &str,
    actions_json: &str,
) -> Promise {
    request_signature_with_nonce(contract, caller, contract_id, actions_json, 1)
}

fn request_signature_with_nonce(
    contract: &mut TradingAccountContract,
    caller: AccountId,
    contract_id: &str,
    actions_json: &str,
    nonce: u64,
) -> Promise {
    request_signature_with(contract, caller, contract_id, actions_json, nonce, None)
}

fn request_signature_with(
    contract: &mut TradingAccountContract,
    caller: AccountId,
    contract_id: &str,
    actions_json: &str,
    nonce: u64,
    quote: Option<SignedQuote>,
) -> Promise {
    call_as(caller);
    contract.request_signature(
        contract_id.parse().unwrap(),
        actions_json.to_string(),
        U64(nonce),
        Base58CryptoHash::from([0u8; 32]),
        test_support::mpc_public_key(),
        "trading-account.near".to_string(),
        quote,
    )
}

/// validate_and_build_actions for `actions_json` sent to `contract_id`, with no quote.
fn build_actions(contract_id: &str, actions_json: &str) -> Result<Vec<OmniAction>, String> {
    build_actions_with(contract_id, actions_json, None)
}

fn build_actions_with(
    contract_id: &str,
    actions_json: &str,
    quote: Option<&QuoteTerms>,
) -> Result<Vec<OmniAction>, String> {
    let actions: Vec<ActionString> = serde_json::from_str(actions_json).unwrap();
    trading_account().validate_and_build_actions(actions, &contract_id.parse().unwrap(), quote)
}

/// The receipts `promise` schedules. A promise is only scheduled when it's dropped.
fn receipts_of(promise: Promise) -> Vec<Receipt> {
    drop(promise);
    get_created_receipts()
}

#[test]
fn test_new_sets_owner_and_signer() {
    let contract = trading_account();
    assert_eq!(contract.get_owner_id(), owner());
    assert_eq!(contract.get_signer_id().as_str(), "v1.signer");
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
fn test_set_signer_id_by_owner() {
    let mut contract = trading_account();
    contract.set_signer_id("new-signer.near".parse().unwrap());
    assert_eq!(contract.get_signer_id().as_str(), "new-signer.near");
}

// The signer decides what gets signed, so only the owner can change it. A bot that could repoint
// it could have anything signed (pen test finding #7).
#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_set_signer_id_rejects_agent() {
    let mut contract = trading_account_with_agent();
    call_as(agent());
    contract.set_signer_id("new-signer.near".parse().unwrap());
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_set_signer_id_rejects_unauthorized_caller() {
    let mut contract = trading_account();
    call_as(stranger());
    contract.set_signer_id("new-signer.near".parse().unwrap());
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

// Deleting the MPC key is the only way to invalidate transactions the bot already had signed
// (pen test findings #5 and #6), and only the owner may do it.
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

#[test]
#[should_panic(expected = "Unauthorized: only agents can request signatures")]
fn test_request_signature_rejects_unauthorized_caller() {
    let mut contract = trading_account();
    let _ = request_signature(&mut contract, stranger(), "intents.near", MT_TRANSFER);
}

// Only agents (the bot) can request signatures, not the owner. That's deliberate: the
// owner has no use for signing through the trading account, so it doesn't get the capability.
#[test]
#[should_panic(expected = "Unauthorized: only agents can request signatures")]
fn test_request_signature_rejects_owner() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        owner(),
        "intents.near",
        r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#,
    );
}

#[test]
#[should_panic(expected = "unknown variant `Sign Message`, expected `FunctionCall`")]
fn test_request_signature_rejects_unknown_action_type() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "intents.near",
        r#"[{"type":"Sign Message","Message":"blah blah blah"}]"#,
    );
}

#[test]
#[should_panic(expected = "ft_withdraw on intents.near is not allowed")]
fn test_request_signature_rejects_disallowed_call() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "intents.near",
        r#"[{"type":"FunctionCall","method_name":"ft_withdraw","args":{"receiver_id":"alice.near","token":"wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"}]"#,
    );
}

// A bare Transfer sends NEAR straight out of the trading account. The bot never needs one, so
// it's rejected even next to an allowed call (pen test finding #2).
#[test]
#[should_panic(expected = "unknown variant `Transfer`, expected `FunctionCall`")]
fn test_request_signature_rejects_transfer() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "intents.near",
        r#"[
            {"type":"FunctionCall","method_name":"mt_transfer","args":{"receiver_id":"alice.near","token_id":"nep141:wrap.near","amount":"1000"},"gas":"30000000000000","deposit":"1"},
            {"type":"Transfer","deposit":"1000000000000000000000000"}
        ]"#,
    );
}

// Validation passes, so the call gets as far as scheduling the MPC signer call. The mocked chain
// reports GasExceeded there (with 100 to 300 Tgas attached), so that panic is the pass condition:
// a validation failure would panic earlier with its own message. test_request_signature_with_stub_signer
// runs a whole call in the sandbox.
#[test]
#[should_panic(expected = "GasExceeded")]
fn test_request_signature_accepts_mt_transfer() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(&mut contract, agent(), "intents.near", MT_TRANSFER);
}

// NEAR only accepts a nonce below the block height × 1,000,000, and a re-added key starts at
// (its block height - 1) × 1,000,000. A higher nonce would make a transaction that becomes valid
// later, and could outlive delete_key + add_full_access_key. Signing only nonces below the
// current block height × 1,000,000 keeps every signed transaction below a re-added key's nonce.
#[test]
#[should_panic(expected = "Invalid nonce")]
fn test_request_signature_rejects_nonce_from_a_future_block() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature_with_nonce(
        &mut contract,
        agent(),
        "intents.near",
        MT_TRANSFER,
        BLOCK_HEIGHT * 1_000_000,
    );
}

// The highest nonce that's valid now passes the check, and fails later on the mocked chain's
// GasExceeded, as in test_request_signature_accepts_mt_transfer.
#[test]
#[should_panic(expected = "GasExceeded")]
fn test_request_signature_accepts_highest_valid_nonce() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature_with_nonce(
        &mut contract,
        agent(),
        "intents.near",
        MT_TRANSFER,
        BLOCK_HEIGHT * 1_000_000 - 1,
    );
}

// FunctionCall is the only action type, and the allowlist only checks FunctionCalls, so every
// other action type must fail to parse. If one were added, serde would list it after
// `FunctionCall` and this would fail.
#[test]
fn test_action_string_accepts_only_function_call() {
    for action_type in [
        "Transfer",
        "AddKey",
        "DeleteKey",
        "DeleteAccount",
        "DeployContract",
        "Stake",
        "CreateAccount",
        "functioncall",
    ] {
        let error = serde_json::from_str::<Vec<ActionString>>(&format!(
            r#"[{{"type":"{}"}}]"#,
            action_type
        ))
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(&format!(
                "unknown variant `{}`, expected `FunctionCall` at",
                action_type
            )),
            "{}",
            error
        );
    }
}

#[test]
fn test_validate_and_build_actions_builds_mt_transfer() {
    let actions = build_actions("intents.near", MT_TRANSFER).unwrap();

    let [OmniAction::FunctionCall(call)] = &actions[..] else {
        panic!("unexpected actions: {:?}", actions);
    };
    assert_eq!(call.method_name, "mt_transfer");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&call.args).unwrap(),
        serde_json::json!({"receiver_id": "alice.near", "token_id": "nep141:wrap.near", "amount": "1000"})
    );
    assert_eq!(call.gas, Gas::from_tgas(30));
    assert_eq!(call.deposit, NearToken::from_yoctonear(1));
}

// The bot only ever signs intents.near::mt_transfer (pen test finding #2). Everything
// the allowlist used to permit is rejected, and so are names one typo or suffix away from it.
#[test]
fn test_validate_and_build_actions_rejects_everything_but_mt_transfer_on_intents() {
    const PREVIOUSLY_ALLOWED_CONTRACTS: [&str; 3] = ["wrap.near", "intents.near", "wrap.testnet"];
    const PREVIOUSLY_ALLOWED_METHODS: [&str; 6] = [
        "add_public_key",
        "ft_transfer_call",
        "near_deposit",
        "mt_transfer_call",
        "mt_transfer",
        "ft_withdraw",
    ];
    let mut calls: Vec<(&str, &str)> = PREVIOUSLY_ALLOWED_CONTRACTS
        .iter()
        .flat_map(|contract| {
            PREVIOUSLY_ALLOWED_METHODS
                .iter()
                .map(move |method| (*contract, *method))
        })
        .filter(|call| *call != ("intents.near", "mt_transfer"))
        .collect();
    assert_eq!(calls.len(), 17);
    calls.extend([
        ("attacker.intents.near", "mt_transfer"),
        ("intents.near.attacker.near", "mt_transfer"),
        ("xintents.near", "mt_transfer"),
        ("intents.testnet", "mt_transfer"),
        ("intents.near", "MT_TRANSFER"),
        ("intents.near", "mt_transfer "),
        ("intents.near", "mt-transfer"),
    ]);

    for (contract, method) in calls {
        let actions_json = serde_json::json!([{
            "type": "FunctionCall",
            "method_name": method,
            "args": {},
            "gas": "30000000000000",
            "deposit": "1",
        }])
        .to_string();
        let error = build_actions(contract, &actions_json).unwrap_err();
        assert!(
            error.contains(&format!("{} on {} is not allowed", method, contract)),
            "{}",
            error
        );
    }
}

// One action per transaction, so a quote authorizes exactly one transfer.
#[test]
fn test_validate_and_build_actions_rejects_anything_but_one_action() {
    let one = &MT_TRANSFER[1..MT_TRANSFER.len() - 1];
    for (actions_json, count) in [("[]".to_string(), 0), (format!("[{},{}]", one, one), 2)] {
        let error = build_actions("intents.near", &actions_json).unwrap_err();
        assert!(
            error.contains(&format!("Exactly one action is required. Got: {}", count)),
            "{}",
            error
        );
    }
}

#[test]
fn test_validate_and_build_actions_rejects_invalid_gas() {
    let error = build_actions(
        "intents.near",
        r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{},"gas":"invalid_gas","deposit":"1"}]"#,
    )
    .unwrap_err();
    assert!(error.contains("Invalid gas format"), "{}", error);
}

#[test]
fn test_validate_and_build_actions_rejects_invalid_deposit() {
    let error = build_actions(
        "intents.near",
        r#"[{"type":"FunctionCall","method_name":"mt_transfer","args":{},"gas":"30000000000000","deposit":"invalid_deposit"}]"#,
    )
    .unwrap_err();
    assert!(error.contains("Invalid deposit format"), "{}", error);
}

// The payload is sha256 of the transaction's borsh bytes, computed outside the contract.
const SIGNATURE_REQUEST_PAYLOAD: &str =
    "9baac0a493bdac2ba1ac8830e178a61c4bed685b2006c66e0ae83adec129aed4";

// The arguments request_signature sends to the MPC signer's `sign`. The domain is always 0
// (secp256k1): the agent can't choose another one.
#[test]
fn test_create_signature_request_always_uses_domain_zero() {
    let tx = omni_transaction::TransactionBuilder::new::<omni_transaction::NEAR>()
        .signer_id("test.near".to_string())
        .signer_public_key(PUBLIC_KEY.to_public_key().unwrap())
        .nonce(1)
        .receiver_id("wrap.near".to_string())
        .block_hash(omni_transaction::near::types::BlockHash([0u8; 32]))
        .actions(vec![])
        .build();
    assert_eq!(
        trading_account().create_signature_request(&tx, "test.trading-account.near".to_string()),
        serde_json::json!({ "request": {
            "payload_v2": { "Ecdsa": SIGNATURE_REQUEST_PAYLOAD },
            "path": "test.trading-account.near",
            "domain_id": 0,
        }})
    );
}

// Pins the bytes that get signed and broadcast. request_signature builds the transaction with
// omni-transaction and passes it to sign_request_callback as JSON, which deserializes it back
// into the same type. The expected values were produced by the audited version
// (omni-transaction 0.2, near-sdk 5.17), so a dependency bump that changes the encoding or
// breaks the JSON round trip fails here. Its actions are built directly: the allowlist no longer
// permits them, but the encoding they pin is the same for mt_transfer.
#[test]
fn test_transaction_bytes_unchanged() {
    use omni_transaction::near::types::{
        BlockHash, FunctionCallAction, Secp256K1Signature, Signature, TransferAction,
    };
    use omni_transaction::{NEAR, TransactionBuilder};

    let mut context = get_context(accounts(1));
    context.current_account_id(
        "implicit_07454f3217b9229ead97798c.auth.peerfolio.near"
            .parse()
            .unwrap(),
    );
    testing_env!(context.build());

    let omni_actions = vec![
        OmniAction::FunctionCall(Box::new(FunctionCallAction {
            method_name: "near_deposit".to_string(),
            args: br#"{"a":[1,2,"x"]}"#.to_vec(),
            gas: Gas::from_tgas(30),
            deposit: NearToken::from_yoctonear(123456789012345678901234567),
        })),
        OmniAction::FunctionCall(Box::new(FunctionCallAction {
            method_name: "ft_transfer_call".to_string(),
            args: b"{}".to_vec(),
            gas: Gas::from_tgas(300),
            deposit: NearToken::from_yoctonear(1),
        })),
        OmniAction::Transfer(TransferAction {
            deposit: NearToken::from_yoctonear(u128::MAX),
        }),
    ];
    let tx = TransactionBuilder::new::<NEAR>()
        .signer_id(near_sdk::env::current_account_id().to_string())
        .signer_public_key(
            "secp256k1:3tFRbMqmoa6AAALMrEFAYCEoHcqKxeW38YptwowBVBtXK1vo36HDbUWuR6EZmoK4JcH6HDkNMGGqP1ouV7VZUWya"
                .to_public_key()
                .unwrap(),
        )
        .nonce(42)
        .receiver_id("wrap.near".to_string())
        .block_hash(BlockHash([7u8; 32]))
        .actions(omni_actions)
        .build();

    let expected_for_signing = "35000000696d706c696369745f3037343534663332313762393232396561643937373938632e617574682e70656572666f6c696f2e6e65617201903a9a9933ed92bdda3fcf30ac999060a5a0fa51c2b6c74838d3029a5aadefe038f7e4a91714f42bb5a2459a0d294be0cd047b4a999d6fd912702470f843271d2a0000000000000009000000777261702e6e656172070707070707070707070707070707070707070707070707070707070707070703000000020c0000006e6561725f6465706f7369740f0000007b2261223a5b312c322c2278225d7d00e057eb481b0000874b9f2ca8f258f1fd1e660000000000021000000066745f7472616e736665725f63616c6c020000007b7d00c06e31d91001000100000000000000000000000000000003ffffffffffffffffffffffffffffffff";
    assert_eq!(hex::encode(tx.build_for_signing()), expected_for_signing);

    // Same round trip as request_signature -> sign_request_callback. The JSON is pinned too: a
    // callback scheduled before an upgrade is deserialized by the new code, so changing this
    // JSON breaks requests in flight during the upgrade.
    let tx_json = serde_json::to_string(&tx).unwrap();
    assert_eq!(
        tx_json,
        r#"{"signer_id":"implicit_07454f3217b9229ead97798c.auth.peerfolio.near","public_key":"secp256k1:3tFRbMqmoa6AAALMrEFAYCEoHcqKxeW38YptwowBVBtXK1vo36HDbUWuR6EZmoK4JcH6HDkNMGGqP1ouV7VZUWya","nonce":42,"receiver_id":"wrap.near","block_hash":"US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx","actions":[{"FunctionCall":{"method_name":"near_deposit","args":"eyJhIjpbMSwyLCJ4Il19","gas":"30000000000000","deposit":"123456789012345678901234567"}},{"FunctionCall":{"method_name":"ft_transfer_call","args":"e30=","gas":"300000000000000","deposit":"1"}},{"Transfer":{"deposit":"340282366920938463463374607431768211455"}}]}"#
    );
    let near_tx: NearTransaction = serde_json::from_str(&tx_json).unwrap();
    assert_eq!(
        hex::encode(near_tx.build_for_signing()),
        expected_for_signing
    );

    let mut signature = [0u8; 65];
    for (i, byte) in signature.iter_mut().enumerate() {
        *byte = i as u8;
    }
    assert_eq!(
        hex::encode(
            near_tx.build_with_signature(Signature::SECP256K1(Secp256K1Signature(signature)))
        ),
        "35000000696d706c696369745f3037343534663332313762393232396561643937373938632e617574682e70656572666f6c696f2e6e65617201903a9a9933ed92bdda3fcf30ac999060a5a0fa51c2b6c74838d3029a5aadefe038f7e4a91714f42bb5a2459a0d294be0cd047b4a999d6fd912702470f843271d2a0000000000000009000000777261702e6e656172070707070707070707070707070707070707070707070707070707070707070703000000020c0000006e6561725f6465706f7369740f0000007b2261223a5b312c322c2278225d7d00e057eb481b0000874b9f2ca8f258f1fd1e660000000000021000000066745f7472616e736665725f63616c6c020000007b7d00c06e31d91001000100000000000000000000000000000003ffffffffffffffffffffffffffffffff01000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40"
    );
}

// sign_request_callback with a response shaped like v1.signer's: it must parse the response,
// rebuild the transaction from request_signature's JSON and attach the signature.
fn callback_setup() -> (TradingAccountContract, NearTransaction, String) {
    callback_setup_with("intents.near", MT_TRANSFER)
}

/// A trading account, the transaction request_signature builds for `actions_json` sent to
/// `receiver`, and the JSON it passes to sign_request_callback.
fn callback_setup_with(
    receiver: &str,
    actions_json: &str,
) -> (TradingAccountContract, NearTransaction, String) {
    let mut context = get_context(accounts(1));
    context.current_account_id(
        "implicit_07454f3217b9229ead97798c.auth.peerfolio.near"
            .parse()
            .unwrap(),
    );
    testing_env!(context.build());
    let contract = TradingAccountContract::new(accounts(1), "v1.signer".parse().unwrap());
    let (tx, tx_json) = test_support::unsigned_tx(receiver, actions_json, 7, [9u8; 32]);
    (contract, tx, tx_json)
}

/// Decodes sign_request_callback's base64 result.
fn decode_signed(signed_base64: String) -> Vec<u8> {
    near_sdk::base64::Engine::decode(
        &near_sdk::base64::engine::general_purpose::STANDARD,
        signed_base64,
    )
    .unwrap()
}

#[test]
fn test_sign_request_callback_attaches_mpc_signature() {
    let (mut contract, tx, tx_json) = callback_setup();
    let (response, expected_signed) = crate::test_support::mpc_sign(&tx);

    let signed = decode_signed(contract.sign_request_callback(Ok(response), tx_json));
    assert_eq!(signed, expected_signed);
}

/// A secp256k1 secret key other than the test MPC key.
const OTHER_SECRET_KEY: [u8; 32] = [0x43; 32];

// The callback only returns a transaction whose signature recovers to the transaction's own public
// key, over that transaction's hash (pen test finding #7). Anything else would be
// rejected at broadcast at best, so it's rejected here.
#[test]
#[should_panic(
    expected = "Invalid signature: recovered key doesn't match the transaction's public key"
)]
fn test_sign_request_callback_rejects_signature_from_another_key() {
    let (mut contract, tx, tx_json) = callback_setup();
    let (response, _) = test_support::mpc_sign_with(&tx, OTHER_SECRET_KEY);
    contract.sign_request_callback(Ok(response), tx_json);
}

#[test]
#[should_panic(
    expected = "Invalid signature: recovered key doesn't match the transaction's public key"
)]
fn test_sign_request_callback_rejects_signature_over_another_transaction() {
    let (mut contract, tx, tx_json) = callback_setup();
    let mut other_tx = tx.clone();
    other_tx.nonce = omni_transaction::near::types::U64(tx.nonce.0 + 1);
    let (response, _) = test_support::mpc_sign(&other_tx);
    contract.sign_request_callback(Ok(response), tx_json);
}

#[test]
#[should_panic(
    expected = "Invalid signature: recovered key doesn't match the transaction's public key"
)]
fn test_sign_request_callback_rejects_flipped_recovery_id() {
    let (mut contract, tx, tx_json) = callback_setup();
    let (mut response, _) = test_support::mpc_sign(&tx);
    response.recovery_id ^= 1;
    contract.sign_request_callback(Ok(response), tx_json);
}

// request_signature accepts an ed25519 `mpc_signer_pk`, but the MPC signs with secp256k1, so such a
// transaction can never carry a valid signature. Signed with the test MPC key, recovery succeeds,
// and only the key check rejects it.
#[test]
#[should_panic(
    expected = "Invalid signature: recovered key doesn't match the transaction's public key"
)]
fn test_sign_request_callback_rejects_ed25519_transaction_key() {
    let (mut contract, mut tx, _) = callback_setup();
    tx.signer_public_key = PUBLIC_KEY.to_public_key().unwrap();
    let tx_json = serde_json::to_string(&tx).unwrap();
    let (response, _) = test_support::mpc_sign(&tx);
    contract.sign_request_callback(Ok(response), tx_json);
}

#[test]
#[should_panic(expected = "Invalid hex in r")]
fn test_sign_request_callback_rejects_malformed_big_r() {
    let (mut contract, tx, tx_json) = callback_setup();
    let (mut response, _) = test_support::mpc_sign(&tx);
    response.big_r.affine_point = "02not hex".to_string();
    contract.sign_request_callback(Ok(response), tx_json);
}

// Pen test finding #8: request_signature used to rewrite each deposit in the
// callback JSON with a plain string replace, so a deposit whose digits start another's (1 and
// 10, 5 and 500) could corrupt the JSON or change a deposit. Every deposit must reach the
// signed transaction exactly as requested, including 0 and u128::MAX.
#[test]
fn test_sign_request_callback_keeps_every_deposit() {
    let cases: [&[u128]; 6] = [
        &[1, 10],
        &[10, 1],
        &[5, 500, 50],
        &[1, 1],
        &[0],
        &[u128::MAX],
    ];
    for deposits in cases {
        let actions_json = serde_json::to_string(
            &deposits
                .iter()
                .map(|deposit| {
                    serde_json::json!({
                        "type": "FunctionCall",
                        "method_name": "mt_transfer",
                        "args": {"receiver_id": "alice.near", "token_id": "nep141:wrap.near", "amount": "1000"},
                        "gas": "30000000000000",
                        "deposit": deposit.to_string(),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let (mut contract, tx, tx_json) = callback_setup_with("intents.near", &actions_json);
        let built: Vec<u128> = tx
            .actions
            .iter()
            .map(|action| match action {
                OmniAction::FunctionCall(call) => call.deposit.as_yoctonear(),
                other => panic!("unexpected action: {:?}", other),
            })
            .collect();
        assert_eq!(built, deposits, "built transaction");

        let (response, expected_signed) = test_support::mpc_sign(&tx);
        let signed = decode_signed(contract.sign_request_callback(Ok(response), tx_json));
        assert_eq!(signed, expected_signed, "deposits {:?}", deposits);
    }
}

#[test]
#[should_panic(expected = "Failed to parse the MPC's Signature response")]
fn test_sign_request_callback_rejects_signer_failure() {
    let (mut contract, _, tx_json) = callback_setup();
    contract.sign_request_callback(Err(near_sdk::PromiseError::Failed), tx_json);
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
    let mut contract = TradingAccountContract::new(implicit_owner(), "v1.signer".parse().unwrap());
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
    let mut contract = TradingAccountContract::new(implicit_owner(), "v1.signer".parse().unwrap());
    contract.add_agent(user(0));
    contract.add_agent(user(1));
    assert_eq!(
        hex::encode(state_bytes(&contract)),
        include_str!("../schema/state_v1.hex").trim()
    );
}

#[test]
fn test_migrate_v0_keeps_owner_signer_and_agents() {
    let raw = write_v0_state(10);
    let mut contract = TradingAccountContract::migrate();

    assert_eq!(contract.get_owner_id(), implicit_owner());
    assert_eq!(contract.get_signer_id().as_str(), "v1.signer");
    assert_eq!(contract.get_agents(), (0..10).map(user).collect::<Vec<_>>());
    assert!(contract.is_agent(user(9)));
    // The set is moved, not rebuilt: the new state is the version byte, then v0's bytes unchanged.
    let migrated = state_bytes(&contract);
    assert_eq!(migrated[0], crate::STATE_VERSION);
    assert_eq!(&migrated[1..], &raw[..]);
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

// --- Outflows: where an mt_transfer may send funds ---

/// The trading account in these tests: the mocked chain's current account.
fn trading_account_id() -> AccountId {
    // VMContextBuilder's default, not accounts(0), which is "alice".
    "alice.near".parse().unwrap()
}

/// The accounts funds may go to without a quote.
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

/// validate_and_build_actions' outflow check for one mt_transfer with `args` and `deposit`.
fn outflow(
    args: serde_json::Value,
    deposit: &str,
    quote: Option<&QuoteTerms>,
) -> Result<(), String> {
    let actions_json = serde_json::json!([{
        "type": "FunctionCall",
        "method_name": "mt_transfer",
        "args": args,
        "gas": "30000000000000",
        "deposit": deposit,
    }])
    .to_string();
    build_actions_with("intents.near", &actions_json, quote).map(drop)
}

fn transfer_to(receiver_id: &str) -> serde_json::Value {
    serde_json::json!({"receiver_id": receiver_id, "token_id": "nep141:wrap.near", "amount": "1000"})
}

// A live quote 1Click returned on 2026-10-22: its signature, and the payload Peerfolio Backend's
// build_signed_payload flattens it to. It proves the contract hashes and verifies exactly as
// 1Click's SDK does, with the compiled-in key.
/// The real quote's recipient and refundTo.
const REAL_QUOTE_ACCOUNT: &str = "implicit_2dde80947980ec0aa6242405.auth.peerfolio.near";

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
fn test_outflow_without_quote_allows_the_trading_account_and_owner() {
    for receiver in ours() {
        outflow(transfer_to(receiver.as_str()), "1", None).unwrap();
    }
}

// The stolen-bot-key case: without a quote, funds can't leave for anyone else.
#[test]
fn test_outflow_without_quote_rejects_other_receivers() {
    for receiver in [
        "attacker.near",
        DEPOSIT_ADDRESS,
        "alice.near.attacker.near",
        "bob.near",
    ] {
        let error = outflow(transfer_to(receiver), "1", None).unwrap_err();
        assert!(
            error.contains("is not the trading account or its owner"),
            "{}",
            error
        );
    }
}

#[test]
fn test_outflow_accepts_every_mt_transfer_argument() {
    let mut args = transfer_to("alice.near");
    args["approval"] = serde_json::json!(null);
    args["memo"] = serde_json::json!("note");
    outflow(args, "1", None).unwrap();
}

// An argument intents.near adds later could redirect funds, so it needs a release first.
#[test]
fn test_outflow_rejects_unknown_arguments() {
    for key in ["msg", "receiver", "recipient"] {
        let mut args = transfer_to("alice.near");
        args[key] = serde_json::json!("attacker.near");
        let error = outflow(args, "1", None).unwrap_err();
        assert!(error.contains("unknown field"), "{}", error);
    }
}

#[test]
fn test_outflow_rejects_missing_receiver() {
    let error = outflow(
        serde_json::json!({"token_id": "nep141:wrap.near", "amount": "1000"}),
        "1",
        None,
    )
    .unwrap_err();
    assert!(error.contains("missing field `receiver_id`"), "{}", error);
}

#[test]
fn test_outflow_requires_one_yocto() {
    for deposit in ["0", "2", "1000000000000000000000000"] {
        let error = outflow(transfer_to("alice.near"), deposit, None).unwrap_err();
        assert!(error.contains("exactly 1 yoctoNEAR"), "{}", error);
    }
}

#[test]
fn test_outflow_with_quote_allows_its_deposit_address() {
    let terms = quote_terms(serde_json::json!({}));
    outflow(transfer_to(DEPOSIT_ADDRESS), "1", Some(&terms)).unwrap();
}

// A liquidation pays out to the owner while refunds come back to the trading account.
#[test]
fn test_outflow_with_quote_allows_the_owner_as_recipient_and_refund() {
    for field in ["recipient", "refundTo"] {
        let terms = quote_terms(serde_json::json!({ field: owner().as_str() }));
        outflow(transfer_to(DEPOSIT_ADDRESS), "1", Some(&terms)).unwrap();
    }
}

#[test]
fn test_outflow_with_quote_rejects_another_receiver() {
    let terms = quote_terms(serde_json::json!({}));
    for receiver in ["attacker.near", "alice.near"] {
        let error = outflow(transfer_to(receiver), "1", Some(&terms)).unwrap_err();
        assert!(
            error.contains("is not the quote's depositAddress"),
            "{}",
            error
        );
    }
}

#[test]
fn test_outflow_with_quote_rejects_another_token() {
    let terms = quote_terms(serde_json::json!({}));
    let mut args = transfer_to(DEPOSIT_ADDRESS);
    args["token_id"] = serde_json::json!("nep141:usdc.near");
    let error = outflow(args, "1", Some(&terms)).unwrap_err();
    assert!(
        error.contains("is not the quote's originAsset"),
        "{}",
        error
    );
}

#[test]
fn test_outflow_accepts_the_maximum_slippage() {
    let terms = quote_terms(serde_json::json!({ "slippageTolerance": quote::MAX_SLIPPAGE_BPS }));
    outflow(transfer_to(DEPOSIT_ADDRESS), "1", Some(&terms)).unwrap();
}

// Each quote term that would send funds, or their refund, anywhere but back to the user.
#[test]
fn test_outflow_rejects_quotes_that_pay_out_elsewhere() {
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
        let terms = quote_terms(changes);
        let error = outflow(transfer_to(DEPOSIT_ADDRESS), "1", Some(&terms)).unwrap_err();
        assert!(error.contains(expected), "{}: {}", expected, error);
    }
}

#[test]
#[should_panic(expected = "is not the trading account or its owner")]
fn test_request_signature_rejects_transfer_to_a_stranger() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "intents.near",
        &MT_TRANSFER.replace("alice.near", "attacker.near"),
    );
}

// request_signature verifies against the compiled-in key, so a test-signed quote is refused.
#[test]
#[should_panic(expected = "Invalid quote signature: not signed by 1Click")]
fn test_request_signature_rejects_a_quote_not_signed_by_1click() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature_with(
        &mut contract,
        agent(),
        "intents.near",
        &MT_TRANSFER.replace("alice.near", DEPOSIT_ADDRESS),
        1,
        Some(sign_quote(&quote_payload(serde_json::json!({})))),
    );
}

/// Makes `caller` the predecessor of the next call, on the trading account at the real quote's
/// recipient.
fn call_real_quote_account_as(caller: AccountId) {
    testing_env!(
        get_context(caller)
            .current_account_id(REAL_QUOTE_ACCOUNT.parse().unwrap())
            .build()
    );
}

// A real quote passes every check and reaches the MPC call, which fails on the mocked chain's
// GasExceeded as in test_request_signature_accepts_mt_transfer.
#[test]
#[should_panic(expected = "GasExceeded")]
fn test_request_signature_accepts_a_real_1click_quote() {
    call_real_quote_account_as(owner());
    let mut contract = TradingAccountContract::new(owner(), "v1.signer".parse().unwrap());
    contract.add_agent(agent());
    call_real_quote_account_as(agent());
    let _ = contract.request_signature(
        "intents.near".parse().unwrap(),
        MT_TRANSFER.replace("alice.near", DEPOSIT_ADDRESS),
        U64(1),
        Base58CryptoHash::from([0u8; 32]),
        test_support::mpc_public_key(),
        "trading-account.near".to_string(),
        Some(real_quote()),
    );
}
