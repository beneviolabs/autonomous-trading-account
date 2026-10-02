use omni_transaction::near::types::U128 as OmniU128;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct SafeU128(pub u128);

impl Serialize for SafeU128 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize large numbers as strings
        serializer.serialize_str(&self.0.to_string())
    }
}

impl From<OmniU128> for SafeU128 {
    fn from(value: OmniU128) -> Self {
        SafeU128(value.0)
    }
}
