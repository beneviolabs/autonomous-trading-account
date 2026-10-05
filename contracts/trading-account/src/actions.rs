use near_sdk::AccountId;

const ALLOWED_CONTRACTS: &[&str] = &["wrap.near", "intents.near", "wrap.testnet"];
const ALLOWED_METHODS: &[&str] = &[
    "add_public_key",
    "ft_transfer_call",
    "near_deposit",
    "mt_transfer_call",
    "mt_transfer",
    "ft_withdraw",
];

/// Checks a function call against the allowlist. Every allowed method is allowed on every allowed
/// contract, and the arguments aren't checked.
pub fn check_allowlist(contract_id: &AccountId, method_name: &str) -> Result<(), String> {
    if !ALLOWED_CONTRACTS.contains(&contract_id.as_str()) {
        return Err(format!(
            "{} is not allowed. Only {:?} are permitted",
            contract_id, ALLOWED_CONTRACTS
        ));
    }
    if !ALLOWED_METHODS.contains(&method_name) {
        return Err(format!(
            "Method {} is restricted. Allowed methods: {:?}",
            method_name, ALLOWED_METHODS
        ));
    }
    Ok(())
}
