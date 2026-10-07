use near_sdk::AccountId;

const ALLOWED_CALLS: &[(&str, &str)] = &[("intents.near", "mt_transfer")];

/// Checks a function call against the allowlist. The call must be one of the listed
/// (contract, method) pairs; the arguments aren't checked.
pub fn check_allowlist(contract_id: &AccountId, method_name: &str) -> Result<(), String> {
    if ALLOWED_CALLS.contains(&(contract_id.as_str(), method_name)) {
        return Ok(());
    }
    Err(format!(
        "{} on {} is not allowed. Allowed calls: {:?}",
        method_name, contract_id, ALLOWED_CALLS
    ))
}
