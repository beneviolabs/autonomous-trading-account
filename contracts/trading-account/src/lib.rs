use near_sdk::base64;
use near_sdk::collections::UnorderedSet;

use near_sdk::json_types::{Base58CryptoHash, U64};
use near_sdk::serde::Deserialize;
use near_sdk::{
    AccountId, AccountIdRef, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PublicKey, env,
    near,
};

use omni_transaction::TransactionBuilder;
use omni_transaction::TxBuilder;
use omni_transaction::near::types::Secp256K1Signature;
use omni_transaction::near::utils::PublicKeyStrExt;
use omni_transaction::{
    NEAR,
    near::NearTransaction,
    near::types::{
        Action as OmniAction, BlockHash as OmniBlockHash,
        FunctionCallAction as OmniFunctionCallAction, PublicKey as OmniPublicKey,
        Secp256K1PublicKey, Signature,
    },
};

pub use crate::models::*;

mod actions;
#[cfg(all(test, feature = "integration-tests"))]
mod integration_tests;
mod models;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod unit_tests;
mod utils;

const GAS_FOR_REQUEST_SIGNATURE: Gas = Gas::from_tgas(100);
const BASE_GAS: Gas = Gas::from_tgas(10); // Base gas for contract execution
const CALLBACK_GAS: Gas = Gas::from_tgas(10); // Gas reserved for callback
const NEAR_MPC_DOMAIN_ID: u32 = 0;
const MAX_AGENTS: u64 = 10; // Maximum number of agents per trading account
const NEAR_INTENTS_ADDRESS: &AccountIdRef = AccountIdRef::new_or_panic("intents.near");

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct TradingAccountContract {
    owner_id: AccountId,
    agents: UnorderedSet<AccountId>,
    signer_id: AccountId,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ActionString {
    FunctionCall {
        method_name: String,
        args: serde_json::Value,
        gas: String,
        deposit: String,
    },
}

#[near]
impl TradingAccountContract {
    #[init]
    pub fn new(owner_id: AccountId, signer_id: AccountId) -> Self {
        assert!(!env::state_exists(), "Contract is already initialized");

        Self {
            owner_id,
            agents: UnorderedSet::new(b"a"),
            signer_id,
        }
    }

    // Owner methods for managing agents
    pub fn add_agent(&mut self, account_id: AccountId) {
        self.assert_owner();

        // Check maximum limit before adding
        assert!(
            self.agents.len() < MAX_AGENTS,
            "Maximum number of agents reached:({}). One must be removed before adding another.",
            MAX_AGENTS
        );

        self.agents.insert(&account_id);
    }

    pub fn remove_agent(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.agents.remove(&account_id);
    }

    pub fn is_agent(&self, account_id: AccountId) -> bool {
        self.agents.contains(&account_id)
    }

    pub fn get_agents(&self) -> Vec<AccountId> {
        self.agents.to_vec()
    }

    pub fn get_owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_signer_id(&self) -> AccountId {
        self.signer_id.clone()
    }

    pub fn set_signer_id(&mut self, signer_id: AccountId) {
        self.assert_owner();
        self.signer_id = signer_id;
    }

    // Helper methods
    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "You have no power here. Only the owner can perform this action."
        );
    }

    /// Validate and build OmniActions from ActionString inputs
    fn validate_and_build_actions(
        &self,
        actions: Vec<ActionString>,
        contract_id: &AccountId,
    ) -> Result<Vec<OmniAction>, String> {
        if actions.is_empty() {
            return Err("Actions cannot be empty. At least one action is required.".to_string());
        }

        actions
            .into_iter()
            .map(|action| match action {
                ActionString::FunctionCall {
                    method_name,
                    args,
                    gas,
                    deposit,
                } => {
                    let gas = Gas::from_gas(gas.parse().map_err(|_| "Invalid gas format")?);
                    let deposit_near = NearToken::from_yoctonear(
                        deposit.parse().map_err(|_| "Invalid deposit format")?,
                    );
                    actions::check_allowlist(contract_id, &method_name)?;

                    let args_bytes = serde_json::to_vec(&args)
                        .map_err(|e| format!("Failed to serialize args: {}", e))?;

                    Ok(OmniAction::FunctionCall(Box::new(OmniFunctionCallAction {
                        method_name,
                        args: args_bytes,
                        gas,
                        deposit: deposit_near,
                    })))
                }
            })
            .collect()
    }

    /// Create signature request from transaction and required parameters
    fn create_signature_request(
        &self,
        tx: &NearTransaction,
        derivation_path: String,
    ) -> serde_json::Value {
        let hashed_payload = utils::hash_payload(&tx.build_for_signing());

        let sign_request = SignRequest {
            payload_v2: EcdsaPayload {
                ecdsa: hex::encode(hashed_payload),
            },
            path: derivation_path,
            domain_id: NEAR_MPC_DOMAIN_ID, // 0 is secp256k1, the only domain the signature check supports
        };

        serde_json::json!({ "request": sign_request })
    }

    // Request a signature from the MPC signer
    #[payable]
    pub fn request_signature(
        &mut self,
        contract_id: AccountId,
        actions_json: String,
        nonce: U64,
        block_hash: Base58CryptoHash,
        mpc_signer_pk: String,
        derivation_path: String,
    ) -> Promise {
        let attached_gas = env::prepaid_gas();
        assert!(
            attached_gas >= GAS_FOR_REQUEST_SIGNATURE,
            "Not enough gas attached. Please attach at least {} TGas. Attached: {} TGas",
            GAS_FOR_REQUEST_SIGNATURE.as_tgas(),
            attached_gas.as_tgas()
        );

        assert!(
            self.agents.contains(&env::predecessor_account_id()),
            "Unauthorized: only agents can request signatures"
        );

        // Parse actions from JSON string
        let actions: Vec<ActionString> = serde_json::from_str(&actions_json).unwrap_or_else(|e| {
            near_sdk::env::panic_str(&format!("Failed to parse actions JSON: {:?}", e))
        });

        near_sdk::env::log_str(&format!(
            "Request received - Contract: {}, Actions: {:?}, Nonce: {}, Block Hash: {:?}",
            contract_id, actions, nonce.0, block_hash
        ));

        // Validate MPC public key early
        let mpc_public_key = match mpc_signer_pk.to_public_key() {
            Ok(pk) => pk,
            Err(e) => {
                near_sdk::env::panic_str(&format!("Invalid MPC public key format: {}", e));
            }
        };

        // Validate and build OmniActions
        let omni_actions = match self.validate_and_build_actions(actions, &contract_id) {
            Ok(actions) => actions,
            Err(e) => {
                near_sdk::env::panic_str(&format!(
                    "Failed to validate and build OmniActions: {:?}",
                    e
                ));
            }
        };

        // construct the entire transaction to be signed
        let tx = TransactionBuilder::new::<NEAR>()
            .signer_id(env::current_account_id().to_string())
            .signer_public_key(mpc_public_key)
            .nonce(nonce.0) // Use the provided nonce
            .receiver_id(contract_id.to_string())
            .block_hash(OmniBlockHash(block_hash.into()))
            .actions(omni_actions.clone())
            .build();

        // Log transaction details
        near_sdk::env::log_str(&format!(
            "Transaction details before signing:
            - Signer ID: {}
            - Receiver ID: {}
            - derivation_path: {}
            - Signer Public Key: {}
            - Number of Actions: {}",
            tx.signer_id,
            tx.receiver_id,
            derivation_path,
            mpc_signer_pk,
            tx.actions.len()
        ));

        // Serialize transaction into a string to pass into callback
        let tx_json_string = serde_json::to_string(&tx)
            .expect("Internal bug: transaction serialization should never fail");

        near_sdk::env::log_str(&format!(
            "Transaction details - Receiver: {}, Signer: {}, Actions: {:?}, Nonce: {}, BlockHash: {:?}",
            contract_id,
            env::current_account_id(),
            omni_actions,
            nonce.0,
            block_hash
        ));

        // Create signature request
        let request_payload = self.create_signature_request(&tx, derivation_path.clone());

        let request_payload_bytes = match near_sdk::serde_json::to_vec(&request_payload) {
            Ok(bytes) => bytes,
            Err(e) => {
                near_sdk::env::panic_str(&format!("Failed to serialize request payload: {}", e));
            }
        };

        let used_gas = near_sdk::env::used_gas();
        let gas_for_signing = attached_gas
            .saturating_sub(BASE_GAS)
            .saturating_sub(used_gas)
            .saturating_sub(CALLBACK_GAS);

        near_sdk::env::log_str(&format!(
            "Used gas: {}, gas reserved for MPC call: {}",
            used_gas.as_tgas(),
            gas_for_signing.as_tgas()
        ));

        // Call MPC requesting a signature for the above txn
        Promise::new(self.signer_id.clone())
            .function_call(
                "sign".to_string(),
                request_payload_bytes,
                env::attached_deposit(),
                gas_for_signing,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .sign_request_callback(tx_json_string),
            )
    }

    pub fn add_full_access_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_owner();
        Promise::new(env::current_account_id()).add_full_access_key(public_key)
    }

    /// Deletes `public_key` from the trading account. Deleting the MPC key invalidates every
    /// transaction it already signed that hasn't been broadcast. Owner only, with exactly 1 yoctoNEAR
    /// attached so the owner must sign with a full-access key.
    #[payable]
    pub fn delete_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_owner();
        near_sdk::assert_one_yocto();
        Promise::new(env::current_account_id()).delete_key(public_key)
    }

    #[payable]
    pub fn add_full_access_key_and_register_with_intents(
        &mut self,
        public_key: PublicKey,
    ) -> Promise {
        assert_eq!(
            env::attached_deposit(),
            NearToken::from_yoctonear(1),
            "This method requires an attached deposit of exactly 1 yoctoNear"
        );
        let request_payload = serde_json::json!({ "public_key": public_key });
        self.add_full_access_key(public_key).then(
            Promise::new(NEAR_INTENTS_ADDRESS.to_owned()).function_call(
                "add_public_key".to_string(),
                near_sdk::serde_json::to_vec(&request_payload)
                    .expect("Failed to serialize public key payload"),
                env::attached_deposit(),
                BASE_GAS,
            ),
        )
    }

    #[private] // Only callable by the contract itself
    pub fn sign_request_callback(
        &mut self,
        #[callback_result] call_result: Result<EcdsaSignatureResponse, PromiseError>,
        tx_json_string: String,
    ) -> String {
        let response = match call_result {
            Ok(response) => {
                near_sdk::env::log_str(&format!(
                    "Parsed the MPC's Signature response: {:?}",
                    response
                ));
                response
            }
            Err(e) => {
                near_sdk::env::panic_str(&format!(
                    "Failed to parse the MPC's Signature response: {:?}",
                    e
                ));
            }
        };

        // Deserialize transaction that we serialized in request_signature
        let near_tx = serde_json::from_str::<NearTransaction>(&tx_json_string)
            .expect("Internal bug: failed to deserialize our own transaction JSON");

        let message_hash = utils::hash_payload(&near_tx.build_for_signing());
        near_sdk::env::log_str(&format!("Message hash: {}", hex::encode(message_hash)));

        // Handle different signature formats
        let omni_signature = {
            near_sdk::env::log_str("Using SECP256K1 signature format");
            // Convert signature components
            let r = hex::decode(&response.big_r.affine_point[2..]).expect("Invalid hex in r");
            let s = hex::decode(&response.s.scalar).expect("Invalid hex in s");
            let v = response.recovery_id;

            // Combine r and s for verification
            let mut signature = Vec::with_capacity(64);
            signature.extend_from_slice(&r);
            signature.extend_from_slice(&s);

            // Verify signature: it must recover to the key the transaction is signed for
            let Some(recovered_key) = self.recover_key(message_hash.to_vec(), signature, v) else {
                near_sdk::env::log_str("Signature verification failed!");
                near_sdk::env::panic_str("Invalid signature: ecrecover failed");
            };
            match &near_tx.signer_public_key {
                OmniPublicKey::SECP256K1(Secp256K1PublicKey(tx_key))
                    if *tx_key == recovered_key =>
                {
                    near_sdk::env::log_str(&format!(
                        "Signature verified! Recovered public key: {}",
                        near_tx.signer_public_key
                    ));
                }
                _ => near_sdk::env::panic_str(
                    "Invalid signature: recovered key doesn't match the transaction's public key",
                ),
            }

            // Add individual bytes together in the correct order
            let mut signature_bytes = [0u8; 65];
            signature_bytes[..32].copy_from_slice(&r);
            signature_bytes[32..64].copy_from_slice(&s);
            signature_bytes[64] = v;

            // Create signature
            Signature::SECP256K1(Secp256K1Signature(signature_bytes))
        };

        near_sdk::env::log_str(&format!("constructed omni signature: {:?}", omni_signature));

        // Add signature to transaction
        let near_tx_signed = near_tx.build_with_signature(omni_signature);

        let base64_tx =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &near_tx_signed);
        near_sdk::env::log_str(&format!("Signed transaction (base64): {}", base64_tx));

        base64_tx
    }

    fn recover_key(&self, hash: Vec<u8>, signature: Vec<u8>, v: u8) -> Option<[u8; 64]> {
        let recovered: Option<[u8; 64]> = env::ecrecover(&hash, &signature, v, true);

        env::log_str(&format!("Hash: {}", hex::encode(&hash)));
        env::log_str(&format!("Signature: {}", hex::encode(&signature)));
        env::log_str(&format!("V: {}", v));

        recovered
    }
}
