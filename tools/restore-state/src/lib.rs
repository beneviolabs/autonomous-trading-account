//! Dev tooling for scripts/dev/restore.sh and scripts/dev/reset-to-v0.sh. Never deployed for real
//! use: each script deploys it, calls one method, and replaces it with the account's code in ONE
//! transaction. The actions share a receipt, so if any fails nothing changes, and the helper never
//! stays on the account.
//!
//! - `restore` writes raw storage entries back (and removes keys added since), so a trading account
//!   can return to a snapshot.
//! - `reset_to_v0` rewrites v1 state into v0's layout.
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

#[derive(near_sdk::serde::Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct ResetToV0Args {
    /// v0's MPC signer, which v1 dropped: v1.signer on mainnet, v1.signer-prod.testnet on testnet.
    signer_id: String,
}

/// Rewrites v1 state into v0's layout, for switching back to the v0 global code in the same
/// transaction. Dev only: the contract itself has no down migration.
///
/// v1's STATE is `[state version 1][owner_id][agents]` and v0's is
/// `[owner_id][authorized_users][signer_id]`. migrate moved the agent set without rebuilding it,
/// so its entries stay as they are; only the version byte goes and the signer comes back. For a
/// later state version, add its step down to v1 here when you need it.
#[unsafe(no_mangle)]
pub extern "C" fn reset_to_v0() {
    env::setup_panic_hook();
    if env::predecessor_account_id() != env::current_account_id() {
        env::panic_str("only the account itself can reset");
    }
    let args: ResetToV0Args = near_sdk::serde_json::from_slice(&env::input().unwrap())
        .unwrap_or_else(|_| env::panic_str("bad args"));
    let state = env::storage_read(b"STATE").unwrap_or_else(|| env::panic_str("no state"));
    match state.first() {
        Some(1) => {}
        Some(64) => env::panic_str("already v0 state"),
        Some(v) => env::panic_str(&format!("state version {v}: add its step down to v1 first")),
        None => env::panic_str("empty state"),
    }
    let mut v0 = state[1..].to_vec();
    v0.extend_from_slice(&(args.signer_id.len() as u32).to_le_bytes());
    v0.extend_from_slice(args.signer_id.as_bytes());
    env::storage_write(b"STATE", &v0);
}
