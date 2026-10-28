//! Dev tooling for scripts/dev/restore.sh. Never deployed for real use.
//!
//! Writes raw storage entries back (and removes keys added since), so a trading account can return
//! to a snapshot. Deploy it, call `restore`, and switch back to the snapshot's code in ONE
//! transaction: the actions share a receipt, so if any fails nothing changes, and the helper never
//! stays on the account.
use near_sdk::base64::{Engine, engine::general_purpose::STANDARD};
use near_sdk::env;

#[derive(near_sdk::serde::Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct Args {
    /// [key, value] pairs, both base64, as `near contract view-storage ... as-json` prints them.
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
