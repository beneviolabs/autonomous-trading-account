#[cfg(test)]
mod tests {

    use crate::TradingAccountFactory;
    use near_sdk::{
        test_utils::{accounts, VMContextBuilder},
        testing_env, AccountId, Gas, NearToken, Promise, PublicKey,
    };
    use std::str::FromStr;

    const ALICE: &str = "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a970f6648cdf0dbd6de";
    const VICTIM: &str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    const ATTACKER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const MIN_DEPOSIT: u128 = 1_000_000; // 1000 yⓃ for global contract

    fn get_context(
        predecessor: AccountId,
        current_account_id: AccountId,
        deposit: Option<NearToken>,
    ) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor)
            .current_account_id(current_account_id)
            .attached_deposit(deposit.unwrap_or(NearToken::from_yoctonear(MIN_DEPOSIT)))
            .prepaid_gas(Gas::from_tgas(150));
        builder
    }

    #[test]
    fn test_factory_initialization() {
        let account = accounts(1);
        let context = get_context(account.clone(), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            account.clone(),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );
        assert_eq!(
            contract.get_signer_contract(),
            "v1.signer-prod.testnet".parse::<AccountId>().unwrap()
        );
        assert_eq!(contract.get_owner_id(), account);
    }

    #[test]
    #[should_panic(expected = "Must attach at least 1000 yⓃ")]
    fn test_insufficient_deposit() {
        let context = get_context(
            accounts(1),
            "factory.testnet".parse().unwrap(),
            Some(NearToken::from_yoctonear(500_000)),
        );
        testing_env!(context.build());

        let mut contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );
        contract.deposit_and_create_proxy_global(ALICE.parse().unwrap());
    }

    #[test]
    fn test_proxy_code_hash() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "mainnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );
        let hash = contract.get_proxy_code_base58_hash();

        assert!(!hash.is_empty(), "Code hash should not be empty");
        assert_eq!(
            hash, "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz",
            "Hash should match the provided Base58 hash"
        );
    }

    #[test]
    fn test_successful_proxy_creation() {
        let mut contract = factory();
        testing_env!(get_context(
            ALICE.parse().unwrap(),
            "factory.testnet".parse().unwrap(),
            Some(NearToken::from_yoctonear(2_000_000)),
        )
        .build());
        let result = contract.create_proxy_global(ALICE.parse().unwrap());

        // Since we can't fully test Promise chain in unit tests,
        // we at least verify the promise was created
        assert!(matches!(result, Promise { .. }));
    }

    #[test]
    fn test_proxy_creation_refund() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let mut contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );
        let result = contract.on_proxy_created(
            accounts(1),
            Err(near_sdk::PromiseError::Failed),
            NearToken::from_yoctonear(2_000_000),
        );

        // Verify refund promise was created
        assert!(matches!(result, Promise { .. }));
    }

    #[test]
    fn test_set_global_code_hash() {
        let context = get_context(accounts(1), accounts(1), None);
        testing_env!(context.build());

        let mut contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Update the global code hash
        contract.set_global_code_hash("CxhHoMAytiy39MSyKCJRksiWXvhYdRYncFpcVWAd4Pbg".to_string());

        // Verify the hash was updated
        let new_hash = contract.get_proxy_code_base58_hash();
        assert_eq!(new_hash, "CxhHoMAytiy39MSyKCJRksiWXvhYdRYncFpcVWAd4Pbg");
    }

    #[test]
    #[should_panic(expected = "Only the contract owner can perform this action.")]
    fn test_set_global_code_hash_unauthorized() {
        let context = get_context(accounts(2), "factory.testnet".parse().unwrap(), None); // Different account
        testing_env!(context.build());

        let mut contract = TradingAccountFactory::new(
            accounts(2),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Switch the caller to a different account (accounts(3)) so the call is unauthorized
        let caller_context = get_context(accounts(3), "factory.testnet".parse().unwrap(), None);
        testing_env!(caller_context.build());

        // This should fail because accounts(3) is not the owner
        contract.set_global_code_hash("NewHash123456789012345678901234567890".to_string());
    }

    #[test]
    fn test_get_proxy_code_hash_hex() {
        let context = get_context(accounts(1), accounts(1), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        let hex_hash = contract.get_proxy_code_hash_hex();
        assert!(!hex_hash.is_empty(), "Hex hash should not be empty");
        assert_eq!(
            hex_hash.len(),
            64,
            "Hex hash should be 32 bytes (64 hex chars)"
        );
    }

    #[test]
    fn test_add_full_access_key() {
        let context = get_context(accounts(1), accounts(1), None);
        testing_env!(context.build());

        let mut contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        let public_key = PublicKey::from_str("ed25519:11111111111111111111111111111111").unwrap();
        let result = contract.add_full_access_key(public_key);

        // Verify promise was created
        assert!(matches!(result, Promise { .. }));
    }

    fn factory() -> TradingAccountFactory {
        testing_env!(get_context(accounts(1), "factory.testnet".parse().unwrap(), None).build());
        TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        )
    }

    fn base_name(contract: &TradingAccountFactory, owner_id: &str) -> String {
        contract.get_base_account_name(&owner_id.parse().unwrap())
    }

    #[test]
    fn test_get_base_account_name_implicit() {
        let contract = factory();
        assert_eq!(
            base_name(&contract, ALICE),
            "implicit_84a19ae54521c69d4baa0885f3319bc8"
        );
        let other = base_name(&contract, VICTIM);
        assert_ne!(other, base_name(&contract, ALICE));
        assert!(other.starts_with("implicit_"));
        assert_eq!(other.len(), 41);
        // Fits under the longest factory account id.
        let full: Result<AccountId, _> = format!("{}.auth.peerfolio.testnet", other).parse();
        assert!(full.is_ok());
    }

    // Only NEAR implicit ids can own a trading account.

    #[test]
    fn test_get_base_account_name_rejects_non_implicit_ids() {
        let contract = factory();
        for owner_id in [
            "alice.near",
            "alice.testnet",
            "sub.alice.near",
            "sub-alice.near",
            // A named account spelled like ALICE's derived name.
            "implicit_84a19ae54521c69d4baa0885f3319bc8.near",
            // ETH implicit.
            "0x06012c8cf97bead5deae237070f9587f8e7a266d",
            // 64 chars but not hex: a valid top-level named account anyone can register.
            "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a970f6648cdf0dbd6dg",
            "invalid_account_format",
        ] {
            // Parse outside catch_unwind so only the contract's own check can pass the test.
            let owner: AccountId = owner_id.parse().unwrap();
            let result = std::panic::catch_unwind(|| contract.get_base_account_name(&owner));
            assert!(result.is_err(), "{} should be rejected", owner_id);
        }
    }

    #[test]
    fn test_uppercase_hex_never_reaches_the_contract() {
        // AccountId rejects uppercase, so JSON args carrying one fail before the method runs.
        let uppercase = "98793CD91A3F870FB126F66285808C7E094AFCFC4EDA8A970F6648CDF0DBD6DE";
        assert!(uppercase.parse::<AccountId>().is_err());
    }

    #[test]
    #[should_panic(expected = "owner_id must be a NEAR implicit account")]
    fn test_create_proxy_global_rejects_named_owner() {
        let mut contract = factory();
        testing_env!(get_context(
            "alice.near".parse().unwrap(),
            "factory.testnet".parse().unwrap(),
            None
        )
        .build());
        contract.create_proxy_global("alice.near".parse().unwrap());
    }

    #[test]
    fn test_owner_id_is_bound_to_the_derived_name() {
        let mut contract = factory();
        testing_env!(get_context(
            VICTIM.parse().unwrap(),
            "factory.testnet".parse().unwrap(),
            None
        )
        .build());
        // The same argument drives both the name and the owner.
        contract.create_proxy_global(VICTIM.parse().unwrap());
        let logs = near_sdk::test_utils::get_logs();
        let expected_account = format!("{}.factory.testnet", base_name(&contract, VICTIM));
        assert!(logs[0].contains(&format!("Account: {}, Owner: {}", expected_account, VICTIM)));
    }

    #[test]
    #[should_panic(expected = "Only owner_id can create its trading account")]
    fn test_create_proxy_global_rejects_non_owner_predecessor() {
        let mut contract = factory();
        testing_env!(get_context(
            ATTACKER.parse().unwrap(),
            "factory.testnet".parse().unwrap(),
            None
        )
        .build());
        contract.create_proxy_global(VICTIM.parse().unwrap());
    }

    #[test]
    #[should_panic(expected = "Only owner_id can create its trading account")]
    fn test_deposit_and_create_proxy_global_rejects_non_owner_predecessor() {
        let mut contract = factory();
        testing_env!(get_context(
            ATTACKER.parse().unwrap(),
            "factory.testnet".parse().unwrap(),
            None
        )
        .build());
        contract.deposit_and_create_proxy_global(VICTIM.parse().unwrap());
    }

    #[test]
    fn test_deposit_and_create_proxy_global_by_owner() {
        let mut contract = factory();
        testing_env!(get_context(
            ALICE.parse().unwrap(),
            "factory.testnet".parse().unwrap(),
            None
        )
        .build());
        contract.deposit_and_create_proxy_global(ALICE.parse().unwrap());
    }

    #[test]
    fn test_new_selects_signer_by_network() {
        testing_env!(get_context(accounts(1), "factory.near".parse().unwrap(), None).build());
        let contract = TradingAccountFactory::new(
            accounts(1),
            "mainnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );
        assert_eq!(contract.get_signer_contract().as_str(), "v1.signer");
        assert_eq!(
            factory().get_signer_contract().as_str(),
            "v1.signer-prod.testnet"
        );
    }

    #[test]
    fn test_new_rejects_unknown_network() {
        for network in ["mainner", "MAINNET", "prod", ""] {
            let result = std::panic::catch_unwind(|| {
                testing_env!(
                    get_context(accounts(1), "factory.near".parse().unwrap(), None).build()
                );
                TradingAccountFactory::new(
                    accounts(1),
                    network.to_string(),
                    "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
                )
            });
            assert!(result.is_err(), "network {:?} should be rejected", network);
        }
    }

    #[test]
    fn test_verify_implicit_base_name_correct() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test with implicit account
        let implicit_account = "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a82f911432ac1b5dffd";
        let owner_id: AccountId = implicit_account.parse().unwrap();
        let expected_base_name = contract.get_base_account_name(&owner_id);

        // Verify correct base name
        let is_valid = contract.verify_implicit_base_name(owner_id, expected_base_name);
        assert!(is_valid, "Correct base name should be valid");
    }

    #[test]
    fn test_verify_implicit_base_name_incorrect() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test with implicit account
        let implicit_account = "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a82f911432ac1b5dffd";
        let owner_id: AccountId = implicit_account.parse().unwrap();

        // Test with wrong base name
        let wrong_base_name = "implicit_wrong123456789".to_string();
        let is_valid = contract.verify_implicit_base_name(owner_id, wrong_base_name);
        assert!(!is_valid, "Wrong base name should be invalid");
    }

    #[test]
    fn test_verify_implicit_base_name_deterministic() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test that verification is deterministic
        let implicit_account = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let owner_id: AccountId = implicit_account.parse().unwrap();
        let base_name = contract.get_base_account_name(&owner_id);

        // Multiple calls should return the same result
        let is_valid1 = contract.verify_implicit_base_name(owner_id.clone(), base_name.clone());
        let is_valid2 = contract.verify_implicit_base_name(owner_id, base_name);
        assert_eq!(is_valid1, is_valid2);
        assert!(is_valid1, "Deterministic verification should be consistent");
    }
}
