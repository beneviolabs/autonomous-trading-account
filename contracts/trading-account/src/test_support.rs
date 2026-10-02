// Shared by the unit and sandbox tests: rebuilds the transaction request_signature builds, and
// signs it the way the MPC signer does, so the callback's ecrecover check passes.
use crate::TradingAccountContract;
use crate::models::SignatureResponse;
use omni_transaction::near::NearTransaction;
use omni_transaction::near::types::{
    Action, BlockHash, Secp256K1Signature, Signature, U128 as OmniU128,
};
use omni_transaction::near::utils::PublicKeyStrExt;
use omni_transaction::{NEAR, TransactionBuilder, TxBuilder};
use secp256k1::{Message, Secp256k1, SecretKey};

const TEST_MPC_SECRET_KEY: [u8; 32] = [0x42; 32];

/// The test MPC key in the `secp256k1:…` form request_signature takes as `mpc_signer_pk`.
pub fn mpc_public_key() -> String {
    let secret = SecretKey::from_slice(&TEST_MPC_SECRET_KEY).unwrap();
    let uncompressed = secret
        .public_key(&Secp256k1::new())
        .serialize_uncompressed();
    format!(
        "secp256k1:{}",
        bs58::encode(&uncompressed[1..]).into_string()
    )
}

/// The transaction request_signature builds, plus the JSON it passes to sign_request_callback.
/// Call inside a testing_env whose current account is the trading account.
pub fn unsigned_tx(
    contract: &TradingAccountContract,
    receiver: &str,
    actions_json: &str,
    nonce: u64,
    block_hash: [u8; 32],
) -> (NearTransaction, String) {
    let actions = serde_json::from_str(actions_json).unwrap();
    let receiver_id = receiver.parse().unwrap();
    let omni_actions = contract
        .validate_and_build_actions(actions, &receiver_id)
        .unwrap();
    let tx = TransactionBuilder::new::<NEAR>()
        .signer_id(near_sdk::env::current_account_id().to_string())
        .signer_public_key(mpc_public_key().to_public_key().unwrap())
        .nonce(nonce)
        .receiver_id(receiver.to_string())
        .block_hash(BlockHash(block_hash))
        .actions(omni_actions.clone())
        .build();
    let deposits: Vec<OmniU128> = omni_actions
        .iter()
        .map(|action| match action {
            Action::FunctionCall(call) => OmniU128(call.deposit.as_yoctonear()),
            Action::Transfer(transfer) => OmniU128(transfer.deposit.as_yoctonear()),
            _ => OmniU128(0),
        })
        .collect();
    let tx_json =
        contract.convert_deposits_to_strings(serde_json::to_string(&tx).unwrap(), &deposits);
    (tx, tx_json)
}

/// What the MPC signer returns for `tx`, and the signed transaction the callback should build.
pub fn mpc_sign(tx: &NearTransaction) -> (SignatureResponse, Vec<u8>) {
    let hash = crate::utils::hash_payload(&tx.build_for_signing());
    let secret = SecretKey::from_slice(&TEST_MPC_SECRET_KEY).unwrap();
    let (recovery_id, rs) = Secp256k1::new()
        .sign_ecdsa_recoverable(&Message::from_slice(&hash).unwrap(), &secret)
        .serialize_compact();
    let v = recovery_id.to_i32() as u8;

    // Same shape as v1.signer's response: big_r is the compressed R point, upper-case hex.
    let response: SignatureResponse = serde_json::from_value(serde_json::json!({
        "scheme": "Secp256k1",
        "big_r": { "affine_point": format!("{:02X}{}", 2 + (v & 1), hex::encode_upper(&rs[..32])) },
        "s": { "scalar": hex::encode_upper(&rs[32..]) },
        "recovery_id": v,
    }))
    .unwrap();

    let mut signature = [0u8; 65];
    signature[..64].copy_from_slice(&rs);
    signature[64] = v;
    let signed = tx.build_with_signature(Signature::SECP256K1(Secp256K1Signature(signature)));
    (response, signed)
}
