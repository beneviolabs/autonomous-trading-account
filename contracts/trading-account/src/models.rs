use near_sdk::{
    AccountId,
    borsh::{self, BorshDeserialize, BorshSerialize},
};
use omni_transaction::near::types::{Action, BlockHash, PublicKey, Signature, U64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EcdsaPayload {
    pub ecdsa: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct BigR {
    pub affine_point: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct ScalarValue {
    pub scalar: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct EcdsaSignatureResponse {
    pub scheme: String,
    pub big_r: BigR,
    pub s: ScalarValue,
    pub recovery_id: u8,
}

// Alias for backwards compatibility and clearer naming
pub type SignatureResponse = EcdsaSignatureResponse;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct SignRequest {
    pub payload_v2: EcdsaPayload,
    pub path: String,
    pub domain_id: u32,
}

// port of the struct from omni-transaction-rs (src/near/near_transaction.rs). Its JSON shape must
// match omni's, since request_signature serializes omni's NearTransaction and
// sign_request_callback deserializes it into this type.
#[derive(Serialize, Deserialize, Debug, Clone, BorshSerialize, BorshDeserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct NearTransaction {
    /// An account on which behalf transaction is signed
    pub signer_id: AccountId,
    /// A public key of the access key which was used to sign an account.
    /// Access key holds permissions for calling certain kinds of actions.
    #[serde(rename = "public_key")]
    pub signer_public_key: PublicKey,
    /// Nonce is used to determine order of transaction in the pool.
    /// It increments for a combination of `signer_id` and `public_key`
    pub nonce: U64,
    /// Receiver account for this transaction
    pub receiver_id: AccountId,
    /// The hash of the block in the blockchain on top of which the given transaction is valid
    pub block_hash: BlockHash,
    /// A list of actions to be applied
    pub actions: Vec<Action>,
}

/// Signed NEAR transaction abstraction
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct SignedTransaction {
    pub transaction: NearTransaction,
    pub signature: Signature,
}

impl NearTransaction {
    pub fn build_for_signing(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("failed to serialize NEAR transaction")
    }

    pub fn build_with_signature(&self, signature: Signature) -> Vec<u8> {
        let signed_tx = SignedTransaction {
            transaction: self.clone(),
            signature,
        };
        borsh::to_vec(&signed_tx).expect("failed to serialize NEAR transaction")
    }

    pub fn from_json(json: &str) -> Result<Self, near_sdk::serde_json::Error> {
        near_sdk::serde_json::from_str(json)
    }
}
