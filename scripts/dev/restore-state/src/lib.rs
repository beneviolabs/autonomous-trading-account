//! Dev helper for scripts/dev/restore.sh and reset-to-v0.sh. Never deployed for real use: each
//! script deploys it, calls `restore`, and puts the account's code back in ONE transaction. The
//! actions share a receipt, so if any fails nothing changes, and the helper never stays on the
//! account.
//!
//! `restore` writes raw storage entries back, and removes keys added since, so a trading account
//! can return to a snapshot. Only the account itself can call it.
use near_sdk::base64::{Engine, engine::general_purpose::STANDARD};
use near_sdk::env;

#[derive(near_sdk::serde::Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct Args {
    /// [key, value] pairs, both base64, from a scripts/dev/snapshot.sh snapshot.
    entries: Vec<(String, String)>,
    /// Keys (base64) written after the snapshot, e.g. an agent added since. Removed.
    #[serde(default)]
    remove: Vec<String>,
}

#[unsafe(no_mangle)]
pub extern "C" fn restore() {
    env::setup_panic_hook();
    if env::predecessor_account_id() != env::current_account_id() {
        env::panic_str("only the account itself can restore");
    }
    let args: Args = near_sdk::serde_json::from_slice(&env::input().unwrap())
        .unwrap_or_else(|_| env::panic_str("bad args"));
    for (k, v) in args.entries {
        let k = STANDARD.decode(k).unwrap_or_else(|_| env::panic_str("bad key"));
        let v = STANDARD.decode(v).unwrap_or_else(|_| env::panic_str("bad value"));
        env::storage_write(&k, &v);
    }
    for k in args.remove {
        let k = STANDARD.decode(k).unwrap_or_else(|_| env::panic_str("bad key"));
        env::storage_remove(&k);
    }
}
