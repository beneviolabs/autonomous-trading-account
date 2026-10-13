use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct SignRequest {
    pub payload_v2: EcdsaPayload,
    pub path: String,
    pub domain_id: u32,
}

/// What contract_version returns.
#[derive(Serialize)]
pub struct ContractVersion {
    pub contract_version: String,
    pub state_version: u8,
}
