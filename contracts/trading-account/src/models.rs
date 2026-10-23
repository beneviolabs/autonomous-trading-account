use serde::Serialize;

/// What contract_version returns.
#[derive(Serialize)]
pub struct ContractVersion {
    pub contract_version: String,
    pub state_version: u8,
}
