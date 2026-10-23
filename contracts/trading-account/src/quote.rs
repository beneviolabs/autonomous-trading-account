use near_sdk::{AccountId, bs58, env};
use serde::Deserialize;
use time::format_description::well_known::{Iso8601, Rfc3339};
use time::{OffsetDateTime, PrimitiveDateTime};

use crate::utils;

/// The keys 1Click signs quotes with, as raw ed25519 bytes. 1Click's TypeScript SDK hardcodes the
/// only one, `ed25519:reYaWhvwu8Jzo3WUM3zhn6VrhuMEF4eADL17qtRVifc` (src/quote-signature.ts). It
/// isn't published anywhere a contract could read, so rotating it means a release, which can carry
/// both keys while owners upgrade.
pub const ONE_CLICK_MANAGER_KEYS: &[[u8; 32]] = &[[
    12, 183, 233, 73, 227, 135, 30, 207, 40, 74, 80, 184, 231, 116, 171, 41, 179, 247, 178, 54, 62,
    57, 140, 239, 29, 242, 233, 68, 87, 183, 233, 67,
]];

/// One more key to trust, `ed25519:<base58>`, compiled in from the ONE_CLICK_TEST_KEY environment
/// variable. Only scripts/test.sh sets it, for the build the sandbox tests run, so they can sign
/// their own quotes. Release builds pass no environment into their container
/// (`passed_env = []`), so they trust 1Click's key alone.
const TEST_KEY: Option<&str> = option_env!("ONE_CLICK_TEST_KEY");

/// The keys swap accepts quotes from: 1Click's, plus TEST_KEY in a test build.
pub fn trusted_keys() -> Vec<[u8; 32]> {
    let mut keys = ONE_CLICK_MANAGER_KEYS.to_vec();
    if let Some(key) = TEST_KEY {
        let key = key
            .strip_prefix("ed25519:")
            .and_then(|encoded| bs58::decode(encoded).into_vec().ok())
            .and_then(|bytes| bytes.try_into().ok())
            .unwrap_or_else(|| {
                env::panic_str("ONE_CLICK_TEST_KEY is not ed25519:<base58 of 32 bytes>")
            });
        keys.push(key);
    }
    keys
}

/// The highest `slippageTolerance` a quote may ask for, in basis points.
pub const MAX_SLIPPAGE_BPS: u32 = 200;

const INTENTS: &str = "INTENTS";

/// A 1Click quote as 1Click signed it.
#[derive(Deserialize, Debug)]
pub struct SignedQuote {
    /// The exact JSON 1Click signed: the flattened `{...quoteRequest, ...quote, timestamp}` object
    /// with sorted keys, as 1Click's SDK builds it. The contract verifies and parses these bytes,
    /// so it never rebuilds the canonical form itself.
    pub payload: String,
    /// The response's `signature`, `ed25519:<base58>`.
    pub signature: String,
}

/// The fields of a signed quote the contract checks. 1Click signs more than these, and the rest
/// are ignored. Duplicate keys are rejected, so a second `recipient` can't slip past the parser.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct QuoteTerms {
    pub dry: bool,
    pub slippage_tolerance: u32,
    pub origin_asset: String,
    // What the quote expects deposited, in the origin asset's smallest unit.
    pub amount_in: String,
    pub deposit_type: String,
    pub recipient: String,
    pub recipient_type: String,
    pub refund_to: String,
    pub refund_type: String,
    // Only a live quote has one.
    pub deposit_address: Option<String>,
    // When the deposit must arrive by: the quote's deposit deadline, which replaces the request's.
    pub deadline: Option<String>,
    // Redirect output to another chain or call. 1Click's SDK drops them from the payload when
    // unset; a null counts as unset too.
    pub virtual_chain_recipient: Option<serde_json::Value>,
    pub virtual_chain_refund_recipient: Option<serde_json::Value>,
    pub custom_recipient_msg: Option<serde_json::Value>,
}

/// Checks that one of `keys` signed `quote`, the way 1Click's SDK signs, and returns its terms.
pub fn verify(quote: &SignedQuote, keys: &[[u8; 32]]) -> Result<QuoteTerms, String> {
    let signature: [u8; 64] = quote
        .signature
        .strip_prefix("ed25519:")
        .and_then(|encoded| bs58::decode(encoded).into_vec().ok())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("Invalid quote signature: expected ed25519:<base58 of 64 bytes>")?;
    // 1Click signs the base58 text of the payload's SHA-256, not the hash bytes.
    let message = bs58::encode(utils::hash_payload(quote.payload.as_bytes())).into_string();
    if !keys
        .iter()
        .any(|key| env::ed25519_verify(&signature, message.as_bytes(), key))
    {
        return Err("Invalid quote signature: not signed by 1Click".to_string());
    }
    serde_json::from_str(&quote.payload).map_err(|e| format!("Invalid quote payload: {}", e))
}

impl QuoteTerms {
    /// Checks that the swap pays out and refunds only to `ours`, inside intents.near, and that the
    /// quote's deadline is after `now_ms` (Unix milliseconds).
    pub fn check(&self, ours: &[AccountId], now_ms: u64) -> Result<(), String> {
        let is_ours = |account: &str| ours.iter().any(|id| id.as_str() == account);
        if self.dry {
            return Err("Quote is a dry quote, which has no deposit address".to_string());
        }
        let deadline = self.deadline.as_deref().ok_or("Quote has no deadline")?;
        if parse_deadline_ms(deadline)? <= now_ms {
            return Err(format!("Quote deadline {} has passed", deadline));
        }
        if !is_ours(&self.recipient) {
            return Err(format!(
                "Quote recipient {} is not the trading account or its owner",
                self.recipient
            ));
        }
        if !is_ours(&self.refund_to) {
            return Err(format!(
                "Quote refundTo {} is not the trading account or its owner",
                self.refund_to
            ));
        }
        for (field, value) in [
            ("depositType", &self.deposit_type),
            ("recipientType", &self.recipient_type),
            ("refundType", &self.refund_type),
        ] {
            if value != INTENTS {
                return Err(format!(
                    "Quote {} must be {}, not {}",
                    field, INTENTS, value
                ));
            }
        }
        for (field, value) in [
            ("virtualChainRecipient", &self.virtual_chain_recipient),
            (
                "virtualChainRefundRecipient",
                &self.virtual_chain_refund_recipient,
            ),
            ("customRecipientMsg", &self.custom_recipient_msg),
        ] {
            if value.is_some() {
                return Err(format!("Quote {} is not allowed", field));
            }
        }
        if self.slippage_tolerance > MAX_SLIPPAGE_BPS {
            return Err(format!(
                "Quote slippageTolerance {} is above the {} bps maximum",
                self.slippage_tolerance, MAX_SLIPPAGE_BPS
            ));
        }
        Ok(())
    }
}

/// Parses an ISO 8601 date-time into Unix milliseconds. One without a time zone is read as UTC. A
/// date alone is rejected.
pub fn parse_deadline_ms(deadline: &str) -> Result<u64, String> {
    let malformed = || format!("Quote deadline {} is not an ISO 8601 date-time", deadline);
    let parsed = OffsetDateTime::parse(deadline, &Iso8601::DEFAULT)
        .or_else(|_| OffsetDateTime::parse(deadline, &Rfc3339))
        .or_else(|e| {
            // PrimitiveDateTime drops any offset it finds, so it may only see text with no zone.
            // Otherwise a zone the parsers above rejected, such as +25:00, would be read as UTC.
            let time = deadline
                .find(['T', 't', ' '])
                .map_or("", |i| &deadline[i..]);
            if time.contains(['Z', 'z', '+', '-']) {
                return Err(e);
            }
            PrimitiveDateTime::parse(deadline, &Iso8601::DEFAULT).map(PrimitiveDateTime::assume_utc)
        })
        .map_err(|_| malformed())?;
    u64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).map_err(|_| malformed())
}
