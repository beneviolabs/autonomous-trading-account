use std::collections::BTreeMap;

use near_sdk::base64;
use near_sdk::borsh::BorshDeserialize;
use near_sdk::collections::UnorderedSet;
use near_sdk::json_types::{Base58CryptoHash, U128};
use near_sdk::{
    AccountId, AccountIdRef, Gas, GasWeight, NearToken, PanicOnDefault, Promise, PublicKey, env,
    near,
};

pub use crate::models::*;
use crate::quote::SignedQuote;

#[cfg(all(test, feature = "integration-tests"))]
mod integration_tests;
mod models;
mod quote;
#[cfg(test)]
mod unit_tests;
mod utils;

const ADD_PUBLIC_KEY_GAS: Gas = Gas::from_tgas(10); // intents.near's add_public_key
const CALLBACK_GAS: Gas = Gas::from_tgas(10); // swap_callback's and transfer_to_owner_callback's gas
// migrate's budget. With 10 agents it burns about 3 Tgas (release-gate tests, 2026-10-08); the
// rest is headroom for later migrations, which chain every version since the account's.
const MIGRATE_GAS: Gas = Gas::from_tgas(50);
const FACTORY_VIEW_GAS: Gas = Gas::from_tgas(5); // The factory's get_proxy_code_base58_hash
const DO_UPGRADE_GAS: Gas = Gas::from_tgas(60); // do_upgrade's own work plus MIGRATE_GAS, which it hands on
const MAX_AGENTS: u64 = 10; // Maximum number of agents per trading account
const NEAR_INTENTS_ADDRESS: &AccountIdRef = AccountIdRef::new_or_panic("intents.near");
// mt_transfer gets at least this much, plus all the gas swap leaves unused.
const MT_TRANSFER_MIN_GAS: Gas = Gas::from_tgas(10); // the live call used 4.09
// Shares of swap's and transfer_to_owner's unused gas: all of it to the intents.near call, none
// to the callback.
const INTENTS_CALL_GETS_UNUSED_GAS: GasWeight = GasWeight(1);
const CALLBACK_GETS_NO_UNUSED_GAS: u64 = 0;
// swap's own work, which burns about 3 Tgas with the real quote.
const SWAP_OWN_GAS: Gas = Gas::from_tgas(10);
// The least prepaid gas swap accepts.
const GAS_FOR_SWAP: Gas =
    Gas::from_tgas(MT_TRANSFER_MIN_GAS.as_tgas() + CALLBACK_GAS.as_tgas() + SWAP_OWN_GAS.as_tgas());
// mt_batch_transfer gets at least BASE + PER_TOKEN for each token, plus all the gas
// transfer_to_owner leaves unused. About twice what the mainnet intents.near wasm burnt in the
// sandbox (2026-10-23): 3.8 Tgas for 1 token, 6.1 for 5, 9.1 for 10.
const MT_BATCH_TRANSFER_BASE_GAS: Gas = Gas::from_tgas(8);
const MT_BATCH_TRANSFER_GAS_PER_TOKEN: Gas = Gas::from_tgas(1);
// transfer_to_owner's own work.
const TRANSFER_TO_OWNER_OWN_GAS: Gas = Gas::from_tgas(10);
// Our cap, which keeps the gas within what was measured. intents.near's own BATCH_MAX_TOKENS (10)
// doesn't apply to a plain mt_batch_transfer, which took 11 tokens in the sandbox.
const MAX_TRANSFER_TOKENS: usize = 10;
const CONTRACT_VERSION: &str = "1.0.0"; // Semver of the interface. Bumps every release.
const STATE_VERSION: u8 = 1; // The shape of STATE. Bumps only when its Borsh bytes change.
// v0 state has no version byte. Its first byte is the low byte of owner_id's length, and every
// v0 owner is a 64-character implicit account.
const V0_OWNER_LEN: u8 = 64;

// migrate tells v0 state apart only while no state version can equal V0_OWNER_LEN. Remove the
// v0 migration, and this assert, once no account runs v0.
const _: () = assert!(STATE_VERSION < V0_OWNER_LEN);

/// The least gas mt_batch_transfer gets when moving `tokens` tokens.
fn mt_batch_transfer_min_gas(tokens: usize) -> Gas {
    MT_BATCH_TRANSFER_BASE_GAS
        .saturating_add(MT_BATCH_TRANSFER_GAS_PER_TOKEN.saturating_mul(tokens as u64))
}

/// The least prepaid gas transfer_to_owner accepts when moving `tokens` tokens.
fn gas_for_transfer_to_owner(tokens: usize) -> Gas {
    mt_batch_transfer_min_gas(tokens)
        .saturating_add(CALLBACK_GAS)
        .saturating_add(TRANSFER_TO_OWNER_OWN_GAS)
}

/// Panics unless at least `needed` gas was prepaid.
fn assert_prepaid_gas(needed: Gas) {
    let prepaid = env::prepaid_gas();
    assert!(
        prepaid >= needed,
        "Not enough gas attached. Please attach at least {} TGas. Attached: {} TGas",
        needed.as_tgas(),
        prepaid.as_tgas()
    );
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct TradingAccountContract {
    // Must stay first; migrate dispatches on this byte.
    state_version: u8,
    owner_id: AccountId,
    agents: UnorderedSet<AccountId>,
}

// v0's layout, kept only so migrate can read v0 state.
#[derive(BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
struct StateV0 {
    owner_id: AccountId,
    authorized_users: UnorderedSet<AccountId>,
    #[allow(dead_code)] // Read only to get past it; migrate drops it.
    signer_id: AccountId,
}

#[near]
impl TradingAccountContract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert!(!env::state_exists(), "Contract is already initialized");

        Self {
            state_version: STATE_VERSION,
            owner_id,
            agents: UnorderedSet::new(b"a"),
        }
    }

    /// The versions compiled into the running code. migrate guarantees the stored state matches.
    pub fn contract_version(&self) -> ContractVersion {
        ContractVersion {
            contract_version: CONTRACT_VERSION.into(),
            state_version: STATE_VERSION,
        }
    }

    /// Brings any older state up to STATE_VERSION. Runs in the same receipt as the code swap, so a
    /// panic here reverts the swap too.
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        let raw =
            env::storage_read(b"STATE").unwrap_or_else(|| env::panic_str("no state to migrate"));
        let (from_state_version, migrated, agents) = match raw[0] {
            STATE_VERSION => {
                let migrated = Self::try_from_slice(&raw)
                    .unwrap_or_else(|_| env::panic_str("state unreadable"));
                let agents = migrated.agents.to_vec();
                (STATE_VERSION, migrated, agents)
            }
            // Must come before the downgrade guard, which would otherwise refuse v0 state.
            V0_OWNER_LEN => {
                let v0 = StateV0::try_from_slice(&raw)
                    .unwrap_or_else(|_| env::panic_str("v0 unreadable"));
                let agents = v0.authorized_users.to_vec();
                // The set is moved, not rebuilt, so its prefix and stored agents don't change.
                // v0's signer_id is dropped: swap needs no MPC signer.
                let migrated = Self {
                    state_version: STATE_VERSION,
                    owner_id: v0.owner_id,
                    agents: v0.authorized_users,
                };
                (0, migrated, agents)
            }
            v if v > STATE_VERSION => env::panic_str("downgrade not supported"),
            _ => env::panic_str("unknown state version"),
        };

        assert_eq!(
            migrated.state_version, STATE_VERSION,
            "migration post-condition"
        );
        // NEP-297. Logs the agents too: they live under their own storage keys, so the STATE
        // bytes alone couldn't restore a set a bad migration lost.
        let event = serde_json::json!({
            "standard": "trading_account",
            "version": "1.0.0",
            "event": "migrated",
            "data": [{
                "from_state_version": from_state_version,
                "to_state_version": STATE_VERSION,
                "state": base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &raw,
                ),
                "agents": agents,
            }],
        });
        env::log_str(&format!("EVENT_JSON:{}", event));

        migrated
    }

    /// Upgrades this account to the code hash its factory points at. The owner passes the hash
    /// they expect, so a pointer moved after they looked can't change what they sign.
    #[payable]
    pub fn upgrade(&mut self, expected_hash: String) -> Promise {
        // One yocto means the owner signed with a full-access key, not a function-call key.
        near_sdk::assert_one_yocto();
        self.assert_owner();

        // Every promise in this chain is returned. A dropped one would let the owner's
        // transaction report success even when a later receipt fails.
        Promise::new(Self::factory_id())
            .function_call(
                "get_proxy_code_base58_hash".to_string(),
                vec![],
                NearToken::from_near(0),
                FACTORY_VIEW_GAS,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(DO_UPGRADE_GAS)
                    .do_upgrade(expected_hash),
            )
    }

    // callback_unwrap, not callback_result: a failed factory view must fail the upgrade.
    #[private]
    pub fn do_upgrade(&self, expected_hash: String, #[callback_unwrap] latest: String) -> Promise {
        assert_eq!(
            latest, expected_hash,
            "factory pointer does not match expected_hash"
        );
        let hash = Base58CryptoHash::try_from(latest.as_str())
            .unwrap_or_else(|_| env::panic_str("invalid code hash"));

        // One batch on this account: the code swap and migrate share a receipt, so a migrate
        // panic reverts the swap. Don't chain migrate with .then: that is a second receipt, so
        // the swap would commit on its own and leave the new code on unmigrated state.
        Promise::new(env::current_account_id())
            .use_global_contract(hash)
            .function_call(
                "migrate".to_string(),
                vec![],
                NearToken::from_near(0),
                MIGRATE_GAS,
            )
    }

    // Owner methods for managing agents
    pub fn add_agent(&mut self, account_id: AccountId) {
        self.assert_owner();

        // Check maximum limit before adding
        assert!(
            self.agents.len() < MAX_AGENTS,
            "Maximum number of agents reached:({}). One must be removed before adding another.",
            MAX_AGENTS
        );

        self.agents.insert(&account_id);
    }

    pub fn remove_agent(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.agents.remove(&account_id);
    }

    pub fn is_agent(&self, account_id: AccountId) -> bool {
        self.agents.contains(&account_id)
    }

    pub fn get_agents(&self) -> Vec<AccountId> {
        self.agents.to_vec()
    }

    pub fn get_owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    // Helper methods
    // The factory only creates direct subaccounts, so the parent is the factory. Derived, never
    // stored or taken from a caller.
    fn factory_id() -> AccountId {
        env::current_account_id()
            .get_parent_account_id()
            .map(AccountIdRef::to_owned)
            .unwrap_or_else(|| env::panic_str("trading account has no parent factory"))
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "You have no power here. Only the owner can perform this action."
        );
    }

    /// Panics unless the caller is the owner or an agent, naming the refused `action`.
    fn assert_owner_or_agent(&self, action: &str) {
        let caller = env::predecessor_account_id();
        if caller != self.owner_id && !self.agents.contains(&caller) {
            env::panic_str(&format!(
                "Unauthorized: only the owner or an agent can {}",
                action
            ));
        }
    }

    /// Deposits into a 1Click quote by calling intents.near's mt_transfer directly, so intents.near
    /// sees the trading account as the sender and no transaction is signed for it. The transfer
    /// comes from the quote alone: its depositAddress, originAsset and amountIn. The owner or an
    /// agent, with exactly 1 yoctoNEAR attached, which is forwarded to mt_transfer, and at least
    /// GAS_FOR_SWAP prepaid. mt_transfer gets all the gas swap doesn't use.
    #[payable]
    pub fn swap(&mut self, quote_payload: String, quote_signature: String) -> Promise {
        near_sdk::assert_one_yocto();
        self.assert_owner_or_agent("swap");
        assert_prepaid_gas(GAS_FOR_SWAP);
        let quote = SignedQuote {
            payload: quote_payload,
            signature: quote_signature,
        };
        let terms =
            quote::verify(&quote, &quote::trusted_keys()).unwrap_or_else(|e| env::panic_str(&e));
        terms
            .check(
                &[env::current_account_id(), self.owner_id.clone()],
                env::block_timestamp_ms(),
            )
            .unwrap_or_else(|e| env::panic_str(&e));
        // check passes only live quotes, and a live quote always has one.
        let deposit_address = terms
            .deposit_address
            .unwrap_or_else(|| env::panic_str("Quote has no depositAddress"));

        let args = serde_json::json!({
            "receiver_id": deposit_address,
            "token_id": terms.origin_asset,
            "amount": terms.amount_in,
        });
        env::log_str(&format!("swap: mt_transfer {}", args));
        Promise::new(NEAR_INTENTS_ADDRESS.to_owned())
            .function_call_weight(
                "mt_transfer".to_string(),
                serde_json::to_vec(&args).expect("Internal bug: args always serialize"),
                env::attached_deposit(),
                MT_TRANSFER_MIN_GAS,
                INTENTS_CALL_GETS_UNUSED_GAS,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .with_unused_gas_weight(CALLBACK_GETS_NO_UNUSED_GAS)
                    .swap_callback(),
            )
    }

    /// swap's callback. Fails the agent's transaction when mt_transfer failed, so the failure
    /// can't be missed.
    #[private]
    pub fn swap_callback(&self) {
        if !near_sdk::is_promise_success() {
            env::panic_str("swap: mt_transfer on intents.near failed");
        }
        env::log_str("swap: mt_transfer succeeded");
    }

    /// Moves `tokens` (token id to amount) from the trading account's intents balance to the
    /// owner's, inside intents, with one mt_batch_transfer, which is atomic: every token moves or
    /// none does. The receiver is always the owner, so there's no allowlist or cap. The owner or
    /// an agent, with exactly 1 yoctoNEAR attached, which is forwarded, and 1 to 10 tokens, none
    /// with a zero amount. mt_batch_transfer gets all the gas this doesn't use.
    #[payable]
    pub fn transfer_to_owner(
        &mut self,
        tokens: BTreeMap<String, U128>,
        memo: Option<String>,
    ) -> Promise {
        near_sdk::assert_one_yocto();
        self.assert_owner_or_agent("transfer to the owner");
        assert!(
            (1..=MAX_TRANSFER_TOKENS).contains(&tokens.len()),
            "tokens must have 1 to {} entries",
            MAX_TRANSFER_TOKENS
        );
        if let Some((token_id, _)) = tokens.iter().find(|(_, amount)| amount.0 == 0) {
            env::panic_str(&format!("Amount of {} must be above 0", token_id));
        }
        assert_prepaid_gas(gas_for_transfer_to_owner(tokens.len()));

        let (token_ids, amounts): (Vec<String>, Vec<U128>) = tokens.into_iter().unzip();
        let args = serde_json::json!({
            "receiver_id": self.owner_id,
            "token_ids": token_ids,
            "amounts": amounts,
            "memo": memo,
        });
        env::log_str(&format!("transfer_to_owner: mt_batch_transfer {}", args));
        Promise::new(NEAR_INTENTS_ADDRESS.to_owned())
            .function_call_weight(
                "mt_batch_transfer".to_string(),
                serde_json::to_vec(&args).expect("Internal bug: args always serialize"),
                env::attached_deposit(),
                mt_batch_transfer_min_gas(token_ids.len()),
                INTENTS_CALL_GETS_UNUSED_GAS,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .with_unused_gas_weight(CALLBACK_GETS_NO_UNUSED_GAS)
                    .transfer_to_owner_callback(),
            )
    }

    /// transfer_to_owner's callback. Fails the caller's transaction when mt_batch_transfer
    /// failed, so the failure can't be missed.
    #[private]
    pub fn transfer_to_owner_callback(&self) {
        if !near_sdk::is_promise_success() {
            env::panic_str("transfer_to_owner: mt_batch_transfer on intents.near failed");
        }
        env::log_str("transfer_to_owner: mt_batch_transfer succeeded");
    }

    pub fn add_full_access_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_owner();
        Promise::new(env::current_account_id()).add_full_access_key(public_key)
    }

    /// Deletes `public_key` from the trading account, such as the MPC key 1.0.0 no longer needs.
    /// Owner only, with exactly 1 yoctoNEAR attached so the owner must sign with a full-access key.
    #[payable]
    pub fn delete_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_owner();
        near_sdk::assert_one_yocto();
        Promise::new(env::current_account_id()).delete_key(public_key)
    }

    #[payable]
    pub fn add_full_access_key_and_register_with_intents(
        &mut self,
        public_key: PublicKey,
    ) -> Promise {
        assert_eq!(
            env::attached_deposit(),
            NearToken::from_yoctonear(1),
            "This method requires an attached deposit of exactly 1 yoctoNear"
        );
        let request_payload = serde_json::json!({ "public_key": public_key });
        self.add_full_access_key(public_key).then(
            Promise::new(NEAR_INTENTS_ADDRESS.to_owned()).function_call(
                "add_public_key".to_string(),
                near_sdk::serde_json::to_vec(&request_payload)
                    .expect("Failed to serialize public key payload"),
                env::attached_deposit(),
                ADD_PUBLIC_KEY_GAS,
            ),
        )
    }
}
