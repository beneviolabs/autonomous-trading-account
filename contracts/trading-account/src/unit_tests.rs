use crate::{ActionString, TradingAccountContract, test_support};
use near_sdk::json_types::{Base58CryptoHash, U64};
use near_sdk::mock::{MockAction, Receipt};
use near_sdk::test_utils::{VMContextBuilder, accounts, get_created_receipts};
use near_sdk::{AccountId, Gas, NearToken, Promise, testing_env};
use omni_transaction::TxBuilder;
use omni_transaction::near::NearTransaction;
use omni_transaction::near::types::Action as OmniAction;
use omni_transaction::near::utils::PublicKeyStrExt;

const PUBLIC_KEY: &str = "ed25519:11111111111111111111111111111111";

fn owner() -> AccountId {
    accounts(1)
}

/// The authorized user in these tests.
fn agent() -> AccountId {
    accounts(2)
}

/// Neither the owner nor an authorized user.
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
    TradingAccountContract::new(owner(), "v1.signer".parse().unwrap())
}

/// A trading account with `agent()` authorized.
fn trading_account_with_agent() -> TradingAccountContract {
    let mut contract = trading_account();
    contract.add_authorized_user(agent());
    contract
}

/// Calls request_signature as `caller`. The tests only vary the receiver and the actions.
fn request_signature(
    contract: &mut TradingAccountContract,
    caller: AccountId,
    contract_id: &str,
    actions_json: &str,
) -> Promise {
    call_as(caller);
    contract.request_signature(
        contract_id.parse().unwrap(),
        actions_json.to_string(),
        U64(1),
        Base58CryptoHash::from([0u8; 32]),
        test_support::mpc_public_key(),
        "trading-account.near".to_string(),
        None,
    )
}

/// validate_and_build_actions for `actions_json` sent to `contract_id`.
fn build_actions(contract_id: &str, actions_json: &str) -> Result<Vec<OmniAction>, String> {
    let actions: Vec<ActionString> = serde_json::from_str(actions_json).unwrap();
    trading_account().validate_and_build_actions(actions, &contract_id.parse().unwrap())
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
    assert!(contract.get_authorized_users().is_empty());
}

#[test]
fn test_add_and_remove_authorized_user() {
    let mut contract = trading_account();

    contract.add_authorized_user(agent());
    assert!(contract.is_authorized(agent()));

    contract.remove_authorized_user(agent());
    assert!(!contract.is_authorized(agent()));
}

#[test]
#[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
fn test_add_authorized_user_rejects_non_owner() {
    let mut contract = trading_account();
    call_as(agent());
    contract.add_authorized_user(stranger());
}

#[test]
fn test_get_authorized_users() {
    let mut contract = trading_account();
    contract.add_authorized_user(agent());
    contract.add_authorized_user(stranger());

    let users = contract.get_authorized_users();
    assert_eq!(users.len(), 2);
    assert!(users.contains(&agent()));
    assert!(users.contains(&stranger()));
}

#[test]
#[should_panic(expected = "Maximum number of authorized users reached:(10)")]
fn test_add_authorized_user_rejects_more_than_max() {
    let mut contract = trading_account();
    for i in 0..10 {
        contract.add_authorized_user(user(i));
    }
    contract.add_authorized_user(user(11));
}

#[test]
fn test_remove_authorized_user_frees_a_slot() {
    let mut contract = trading_account();
    for i in 0..10 {
        contract.add_authorized_user(user(i));
    }
    assert_eq!(contract.get_authorized_users().len(), 10);

    contract.remove_authorized_user(user(0));
    assert_eq!(contract.get_authorized_users().len(), 9);

    contract.add_authorized_user(user(11));
    assert_eq!(contract.get_authorized_users().len(), 10);
    assert!(contract.is_authorized(user(11)));
}

#[test]
fn test_set_signer_id_by_owner() {
    let mut contract = trading_account();
    contract.set_signer_id("new-signer.near".parse().unwrap());
    assert_eq!(contract.get_signer_id().as_str(), "new-signer.near");
}

#[test]
fn test_set_signer_id_by_authorized_user() {
    let mut contract = trading_account_with_agent();
    call_as(agent());
    contract.set_signer_id("new-signer.near".parse().unwrap());
    assert_eq!(contract.get_signer_id().as_str(), "new-signer.near");
}

#[test]
#[should_panic(expected = "Unauthorized: only authorized users can set signer ID")]
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

#[test]
#[should_panic(expected = "Unauthorized: only authorized users can request signatures")]
fn test_request_signature_rejects_unauthorized_caller() {
    let mut contract = trading_account();
    let _ = request_signature(
        &mut contract,
        stranger(),
        "wrap.near",
        r#"[{"type":"FunctionCall","method_name":"near_deposit","args":{},"gas":"30000000000000","deposit":"1"}]"#,
    );
}

#[test]
#[should_panic(expected = "unknown variant `Sign Message`, expected `FunctionCall` or `Transfer`")]
fn test_request_signature_rejects_unknown_action_type() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "wrap.near",
        r#"[{"type":"Sign Message","Message":"blah blah blah"}]"#,
    );
}

#[test]
#[should_panic(
    expected = "Transfer actions must be accompanied by at least one FunctionCall action"
)]
fn test_request_signature_rejects_lone_transfer() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "bad-account.near",
        r#"[{"type":"Transfer","deposit":"1000000000000000000000000"}]"#,
    );
}

#[test]
#[should_panic(
    expected = "Transfer actions must be accompanied by at least one FunctionCall action"
)]
fn test_request_signature_rejects_transfers_without_function_call() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "wrap.near",
        r#"[{"type":"Transfer","deposit":"1000000000000000000000000"},{"type":"Transfer","deposit":"2000000000000000000000000"}]"#,
    );
}

// Validation passes, so the call gets as far as scheduling the MPC signer call. The mocked chain
// reports GasExceeded there (with 100 to 300 Tgas attached), so that panic is the pass condition:
// a validation failure would panic earlier with its own message. test_request_signature_with_stub_signer
// runs a whole call in the sandbox.
#[test]
#[should_panic(expected = "GasExceeded")]
fn test_request_signature_accepts_transfer_with_function_call() {
    let mut contract = trading_account_with_agent();
    let _ = request_signature(
        &mut contract,
        agent(),
        "wrap.near",
        r#"[
            {"type":"FunctionCall","method_name":"ft_transfer_call","args":{"receiver_id":"alice.near","amount":"1000000000000000000000000"},"gas":"100000000000000","deposit":"1000000000000000000000000"},
            {"type":"Transfer","deposit":"1000000000000000000000000"}
        ]"#,
    );
}

#[test]
fn test_validate_and_build_actions_accepts_function_call() {
    let actions = build_actions(
        "wrap.near",
        r#"[{"type":"FunctionCall","method_name":"ft_transfer_call","args":{"receiver_id":"alice.near","amount":"1000000000000000000000000"},"gas":"100000000000000","deposit":"1000000000000000000000000"}]"#,
    )
    .unwrap();
    assert_eq!(actions.len(), 1);
}

#[test]
fn test_validate_and_build_actions_accepts_transfer_with_function_call() {
    let actions = build_actions(
        "wrap.near",
        r#"[
            {"type":"Transfer","deposit":"500000000000000000000000"},
            {"type":"FunctionCall","method_name":"ft_transfer_call","args":{"receiver_id":"alice.near","amount":"1000000000000000000000000"},"gas":"100000000000000","deposit":"1000000000000000000000000"}
        ]"#,
    )
    .unwrap();
    assert_eq!(actions.len(), 2);
}

#[test]
fn test_validate_and_build_actions_accepts_multiple_actions() {
    let actions = build_actions(
        "wrap.near",
        r#"[
            {"type":"FunctionCall","method_name":"ft_transfer_call","args":{"receiver_id":"alice.near"},"gas":"100000000000000","deposit":"1000000000000000000000000"},
            {"type":"Transfer","deposit":"500000000000000000000000"},
            {"type":"FunctionCall","method_name":"near_deposit","args":{},"gas":"50000000000000","deposit":"0"}
        ]"#,
    )
    .unwrap();
    assert_eq!(actions.len(), 3);
}

#[test]
fn test_validate_and_build_actions_rejects_contract_outside_allowlist() {
    let error = build_actions(
        "disallowed.near",
        r#"[{"type":"FunctionCall","method_name":"ft_transfer_call","args":{},"gas":"100000000000000","deposit":"1000000000000000000000000"}]"#,
    )
    .unwrap_err();
    assert!(error.contains("is not allowed"), "{}", error);
}

#[test]
fn test_validate_and_build_actions_rejects_method_outside_allowlist() {
    let error = build_actions(
        "wrap.near",
        r#"[{"type":"FunctionCall","method_name":"disallowed_method","args":{},"gas":"100000000000000","deposit":"1000000000000000000000000"}]"#,
    )
    .unwrap_err();
    assert!(
        error.contains("Method disallowed_method is restricted"),
        "{}",
        error
    );
}

#[test]
fn test_validate_and_build_actions_rejects_invalid_gas() {
    let error = build_actions(
        "wrap.near",
        r#"[{"type":"FunctionCall","method_name":"ft_transfer_call","args":{},"gas":"invalid_gas","deposit":"1000000000000000000000000"}]"#,
    )
    .unwrap_err();
    assert!(error.contains("Invalid gas format"), "{}", error);
}

#[test]
fn test_validate_and_build_actions_rejects_invalid_deposit() {
    let error = build_actions(
        "wrap.near",
        r#"[{"type":"FunctionCall","method_name":"ft_transfer_call","args":{},"gas":"100000000000000","deposit":"invalid_deposit"}]"#,
    )
    .unwrap_err();
    assert!(error.contains("Invalid deposit format"), "{}", error);
}

#[test]
fn test_validate_and_build_actions_rejects_empty_actions() {
    let error = build_actions("wrap.near", "[]").unwrap_err();
    assert!(error.contains("Actions cannot be empty"), "{}", error);
}

/// The arguments request_signature sends to the MPC signer's `sign`, for a fixed transaction.
fn signature_request(domain_id: Option<u32>) -> serde_json::Value {
    let tx = omni_transaction::TransactionBuilder::new::<omni_transaction::NEAR>()
        .signer_id("test.near".to_string())
        .signer_public_key(PUBLIC_KEY.to_public_key().unwrap())
        .nonce(1)
        .receiver_id("wrap.near".to_string())
        .block_hash(omni_transaction::near::types::BlockHash([0u8; 32]))
        .actions(vec![])
        .build();
    trading_account().create_signature_request(
        &tx,
        "test.trading-account.near".to_string(),
        domain_id,
    )
}

// The payload is sha256 of the transaction's borsh bytes, computed outside the contract.
const SIGNATURE_REQUEST_PAYLOAD: &str =
    "9baac0a493bdac2ba1ac8830e178a61c4bed685b2006c66e0ae83adec129aed4";

#[test]
fn test_create_signature_request_passes_domain_id() {
    assert_eq!(
        signature_request(Some(1)),
        serde_json::json!({ "request": {
            "payload_v2": { "Ecdsa": SIGNATURE_REQUEST_PAYLOAD },
            "path": "test.trading-account.near",
            "domain_id": 1,
        }})
    );
}

#[test]
fn test_create_signature_request_defaults_domain_id_to_zero() {
    assert_eq!(
        signature_request(None),
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
// breaks the JSON round trip fails here.
#[test]
fn test_transaction_bytes_unchanged() {
    use omni_transaction::near::types::{BlockHash, Secp256K1Signature, Signature};
    use omni_transaction::{NEAR, TransactionBuilder};

    let mut context = get_context(accounts(1));
    context.current_account_id(
        "implicit_07454f3217b9229ead97798c.auth.peerfolio.near"
            .parse()
            .unwrap(),
    );
    testing_env!(context.build());
    let contract = TradingAccountContract::new(accounts(1), "v1.signer".parse().unwrap());

    let actions: Vec<ActionString> = serde_json::from_str(
        r#"[
            {"type":"FunctionCall","method_name":"near_deposit","args":{"a":[1,2,"x"]},"gas":"30000000000000","deposit":"123456789012345678901234567"},
            {"type":"FunctionCall","method_name":"ft_transfer_call","args":{},"gas":"300000000000000","deposit":"1"},
            {"type":"Transfer","deposit":"340282366920938463463374607431768211455"}
        ]"#,
    )
    .unwrap();
    let contract_id: AccountId = "wrap.near".parse().unwrap();
    let omni_actions = contract
        .validate_and_build_actions(actions, &contract_id)
        .unwrap();
    let tx = TransactionBuilder::new::<NEAR>()
        .signer_id(near_sdk::env::current_account_id().to_string())
        .signer_public_key(
            "secp256k1:3tFRbMqmoa6AAALMrEFAYCEoHcqKxeW38YptwowBVBtXK1vo36HDbUWuR6EZmoK4JcH6HDkNMGGqP1ouV7VZUWya"
                .to_public_key()
                .unwrap(),
        )
        .nonce(42)
        .receiver_id(contract_id.to_string())
        .block_hash(BlockHash([7u8; 32]))
        .actions(omni_actions)
        .build();

    let expected_for_signing = "35000000696d706c696369745f3037343534663332313762393232396561643937373938632e617574682e70656572666f6c696f2e6e65617201903a9a9933ed92bdda3fcf30ac999060a5a0fa51c2b6c74838d3029a5aadefe038f7e4a91714f42bb5a2459a0d294be0cd047b4a999d6fd912702470f843271d2a0000000000000009000000777261702e6e656172070707070707070707070707070707070707070707070707070707070707070703000000020c0000006e6561725f6465706f7369740f0000007b2261223a5b312c322c2278225d7d00e057eb481b0000874b9f2ca8f258f1fd1e660000000000021000000066745f7472616e736665725f63616c6c020000007b7d00c06e31d91001000100000000000000000000000000000003ffffffffffffffffffffffffffffffff";
    assert_eq!(hex::encode(tx.build_for_signing()), expected_for_signing);

    // Same round trip as request_signature -> sign_request_callback.
    let tx_json = serde_json::to_string(&tx).unwrap();
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
    callback_setup_with(
        "wrap.near",
        r#"[{"type":"FunctionCall","method_name":"near_deposit","args":{},"gas":"30000000000000","deposit":"50000000000000000000000"},{"type":"Transfer","deposit":"1"}]"#,
    )
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
    let (tx, tx_json) = test_support::unsigned_tx(&contract, receiver, actions_json, 7, [9u8; 32]);
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

// Pen test finding #8 (ft-core#1700): request_signature used to rewrite each deposit in the
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
