use near_sdk::serde::de::IgnoredAny;
use near_sdk::{AccountId, NearToken};
use omni_transaction::near::types::FunctionCallAction;
use serde::Deserialize;

use crate::quote::QuoteTerms;

const ALLOWED_CALLS: &[(&str, &str)] = &[("intents.near", "mt_transfer")];

/// Checks a function call against the allowlist. The call must be one of the listed
/// (contract, method) pairs; the arguments aren't checked here, check_outflow checks them.
pub fn check_allowlist(contract_id: &AccountId, method_name: &str) -> Result<(), String> {
    if ALLOWED_CALLS.contains(&(contract_id.as_str(), method_name)) {
        return Ok(());
    }
    Err(format!(
        "{} on {} is not allowed. Allowed calls: {:?}",
        method_name, contract_id, ALLOWED_CALLS
    ))
}

/// intents.near's mt_transfer arguments. Unknown keys are rejected, so an argument intents.near
/// adds later can't redirect funds without a contract release.
/// Only the fields check_outflow reads are kept; the rest are listed so they're allowed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct MtTransferArgs {
    receiver_id: AccountId,
    token_id: String,
    amount: IgnoredAny,
    approval: Option<IgnoredAny>,
    memo: Option<IgnoredAny>,
}

/// Checks where an mt_transfer sends funds. Without a quote it may only go to `ours` (the trading
/// account or its owner). With one, the quote must pay out and refund to `ours`, and the transfer
/// must be its deposit for its origin asset.
pub fn check_outflow(
    call: &FunctionCallAction,
    quote: Option<&QuoteTerms>,
    ours: &[AccountId],
) -> Result<(), String> {
    if call.deposit != NearToken::from_yoctonear(1) {
        return Err("mt_transfer must attach exactly 1 yoctoNEAR".to_string());
    }
    let args: MtTransferArgs = serde_json::from_slice(&call.args)
        .map_err(|e| format!("Invalid mt_transfer args: {}", e))?;

    let Some(quote) = quote else {
        if ours.contains(&args.receiver_id) {
            return Ok(());
        }
        return Err(format!(
            "receiver_id {} is not the trading account or its owner. Sending elsewhere needs a 1Click quote",
            args.receiver_id
        ));
    };
    quote.check(ours)?;
    if quote.deposit_address.as_deref() != Some(args.receiver_id.as_str()) {
        return Err(format!(
            "receiver_id {} is not the quote's depositAddress",
            args.receiver_id
        ));
    }
    if args.token_id != quote.origin_asset {
        return Err(format!(
            "token_id {} is not the quote's originAsset {}",
            args.token_id, quote.origin_asset
        ));
    }
    Ok(())
}
