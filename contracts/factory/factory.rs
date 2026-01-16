use bs58;
use near_sdk::serde::Serialize;
use near_sdk::{
    env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PublicKey,
};

const TESTNET_SIGNER: &str = "v1.signer-prod.testnet";
const MAINNET_SIGNER: &str = "v1.signer";

mod unit_tests;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct TradingAccountFactory {
    signer_contract: AccountId,
    global_proxy_base58_hash: Vec<u8>,
    owner_id: AccountId,
}

#[near]
impl TradingAccountFactory {
    #[init]
    pub fn new(owner_id: AccountId, network: String, global_proxy_base58_hash: String) -> Self {
        assert!(!env::state_exists(), "Already initialized");

        let signer_contract = match network.as_str() {
            "mainnet" => MAINNET_SIGNER,
            _ => TESTNET_SIGNER,
        }
        .parse()
        .unwrap();

        Self {
            signer_contract,
            global_proxy_base58_hash: Self::decode_code_hash(&global_proxy_base58_hash),
            owner_id,
        }
    }

    #[payable]
    pub fn deposit_and_create_proxy_global(&mut self, owner_id: AccountId) -> Promise {
        let deposit = env::attached_deposit();
        assert!(
            deposit >= NearToken::from_yoctonear(1_000_000),
            "Must attach at least 1000 yⓃ" // TODO: make this amount more precise - how much of a deposit must one add? 0.0025?
        );

        self.create_proxy_global(owner_id.clone()).then(
            Self::ext(env::current_account_id())
                .on_proxy_created(env::predecessor_account_id(), deposit),
        )
    }

    #[payable]
    pub fn create_proxy_global(&mut self, owner_id: AccountId) -> Promise {
        let trimmed_owner = self.get_base_account_name(&owner_id);
        let full_sub_account: AccountId =
            format!("{}.{}", trimmed_owner, env::current_account_id())
                .parse()
                .unwrap();

        env::log_str(&format!(
            "Creating proxy with global contract - Account: {}, Owner: {}, Signer: {}, bs58 Code Hash: {}",
            full_sub_account,
            owner_id,
            self.signer_contract,
            bs58::encode(&self.global_proxy_base58_hash).into_string()
        ));

        Promise::new(full_sub_account.clone())
            .create_account()
            .transfer(env::attached_deposit())
            .use_global_contract(self.global_proxy_base58_hash.clone())
            .function_call(
                "new".to_string(),
                near_sdk::serde_json::to_vec(&ProxyInitArgs {
                    owner_id,
                    signer_id: self.signer_contract.clone(),
                })
                .unwrap(),
                NearToken::from_near(0),
                Gas::from_tgas(50),
            )
    }

    #[private]
    pub fn on_proxy_created(
        &mut self,
        original_caller: AccountId,
        #[callback_result] creation_result: Result<(), PromiseError>,
        deposit: NearToken,
    ) -> Promise {
        if creation_result.is_err() {
            env::log_str("Proxy creation failed, refunding deposit");
            Promise::new(original_caller).transfer(deposit)
        } else {
            env::log_str("Proxy created successfully");
            Promise::new(env::current_account_id())
        }
    }

    pub fn get_base_account_name(&self, owner_id: &AccountId) -> String {
        let account_str = owner_id.as_str();

        if account_str.ends_with(".testnet") || account_str.ends_with(".near") {
            // Extract the account name
            let domain_start = if account_str.ends_with(".testnet") {
                account_str.len() - 8 // ".testnet".len()
            } else {
                account_str.len() - 5 // ".near".len()
            };

            let account_part = &account_str[..domain_start];

            // Replace dots with hyphens for subaccounts
            account_part.replace('.', "-")
        } else if account_str.len() == 64 {
            // Implicit account: take the first 32 chars
            let hash_input = &account_str[..32];
            // hash them and encode first 12 bytes as hex to get 24-char result
            let hash = env::sha256(hash_input.as_bytes());
            let truncated = hex::encode(&hash[..12]); // 24 chars (12 bytes * 2)

            // with this approach, we would need ~2^36 (68 billion) implicit accounts to have a
            // 50% chance of collision between base account names.
            // Even with millions of implicit accounts, collision risk is negligible
            format!("implicit_{}", truncated)
        } else {
            near_sdk::env::panic_str(
                "Unsupported account name format for base account name extraction",
            );
        }
    }

    /// Utility method to check if a given implicit base account name corresponds to a specific owner_id
    pub fn verify_implicit_base_name(&self, owner_id: AccountId, base_name: String) -> bool {
        let expected_base_name = self.get_base_account_name(&owner_id);
        expected_base_name == base_name
    }

    /// Helper function to decode Base58 code hash string to 32-byte hash
    fn decode_code_hash(code_hash_str: &str) -> Vec<u8> {
        let decoded_hash = bs58::decode(code_hash_str).into_vec().unwrap_or_else(|e| {
            near_sdk::env::panic_str(&format!("Failed to decode Base58 code hash: {}", e))
        });

        // Verify it's exactly 32 bytes (SHA-256 hash)
        assert_eq!(decoded_hash.len(), 32, "Code hash must be exactly 32 bytes");

        decoded_hash
    }

    /// Set global code hash from Base58 string
    pub fn set_global_code_hash(&mut self, code_hash_str: String) {
        self.assert_owner();

        self.global_proxy_base58_hash = Self::decode_code_hash(&code_hash_str);

        env::log_str(&format!(
            "Global proxy code hash updated to: {}",
            code_hash_str
        ));
    }

    pub fn add_full_access_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_owner();
        Promise::new(env::current_account_id()).add_full_access_key(public_key)
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "Only the contract owner can perform this action."
        );
    }

    // View methods
    pub fn get_owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_proxy_code_base58_hash(&self) -> String {
        bs58::encode(&self.global_proxy_base58_hash).into_string()
    }

    pub fn get_proxy_code_hash_hex(&self) -> String {
        hex::encode(&self.global_proxy_base58_hash)
    }

    pub fn get_signer_contract(&self) -> AccountId {
        self.signer_contract.clone()
    }
}

#[cfg(test)]
impl TradingAccountFactory {
    pub(crate) fn test_get_base_account_name(&self, owner_id: &AccountId) -> String {
        self.get_base_account_name(owner_id)
    }
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct ProxyInitArgs {
    owner_id: AccountId,
    signer_id: AccountId,
}
