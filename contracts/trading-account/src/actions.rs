use near_sdk::{AccountId, Gas, NearToken, near};

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct NearAction {
    pub method_name: Option<String>,
    pub contract_id: AccountId,
    pub gas_attached: Gas,
    pub deposit_attached: NearToken,
}
#[derive(Debug, PartialEq)]
pub enum ActionValidationError {
    ContractNotAllowed(String),
    MethodNotAllowed(String),
}

const ALLOWED_CONTRACTS: &[&str] = &["wrap.near", "intents.near", "wrap.testnet"];
const ALLOWED_METHODS: &[&str] = &[
    "add_public_key",
    "ft_transfer_call",
    "near_deposit",
    "mt_transfer_call",
    "mt_transfer",
    "ft_withdraw",
];

impl NearAction {
    pub fn is_allowed(&self) -> Result<(), ActionValidationError> {
        // Check if contract address is allowed
        let contract_str = self.contract_id.as_str();
        if !ALLOWED_CONTRACTS.contains(&contract_str) {
            return Err(ActionValidationError::ContractNotAllowed(format!(
                "{} is not allowed. Only {:?} are permitted",
                self.contract_id, ALLOWED_CONTRACTS
            )));
        }
        // Check if method is allowed
        if let Some(method) = &self.method_name {
            if !ALLOWED_METHODS.contains(&method.as_str()) {
                return Err(ActionValidationError::MethodNotAllowed(format!(
                    "Method {} is restricted. Allowed methods: {:?}",
                    method, ALLOWED_METHODS
                )));
            }
        }
        Ok(())
    }
}
