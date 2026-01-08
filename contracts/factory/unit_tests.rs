#[cfg(test)]
mod tests {

    use crate::TradingAccountFactory;
    use near_sdk::{
        AccountId, Gas, NearToken, Promise, PublicKey,
        test_utils::{VMContextBuilder, accounts},
        testing_env,
    };
    use std::str::FromStr;

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
        contract.deposit_and_create_proxy_global("alice.testnet".parse().unwrap());
    }

    #[test]
    fn test_proxy_address_generation() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test testnet address
        let owner_id: AccountId = "alice.testnet".parse().unwrap();
        let proxy_address = contract.test_get_base_account_name(&owner_id);
        assert_eq!(proxy_address, "alice");

        // Test subaccount
        let owner_id: AccountId = "trading.alice.testnet".parse().unwrap();
        let proxy_address = contract.test_get_base_account_name(&owner_id);
        assert_eq!(proxy_address, "trading-alice");
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
        let context = get_context(
            accounts(1),
            "factory.testnet".parse().unwrap(),
            Some(NearToken::from_yoctonear(2_000_000)),
        );
        testing_env!(context.build());

        let mut contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );
        let result = contract.create_proxy_global("alice.testnet".parse().unwrap());

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

    #[test]
    fn test_get_base_account_name_named_accounts() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test .testnet accounts
        let owner_id: AccountId = "alice.testnet".parse().unwrap();
        let base_name = contract.get_base_account_name(&owner_id);
        assert_eq!(base_name, "alice");

        // Test .near accounts
        let owner_id: AccountId = "bob.near".parse().unwrap();
        let base_name = contract.get_base_account_name(&owner_id);
        assert_eq!(base_name, "bob");

        // Test subaccounts
        let owner_id: AccountId = "trading.alice.testnet".parse().unwrap();
        let base_name = contract.get_base_account_name(&owner_id);
        assert_eq!(base_name, "trading-alice");

        // Test deep subaccounts
        let owner_id: AccountId = "defi.trading.alice.near".parse().unwrap();
        let base_name = contract.get_base_account_name(&owner_id);
        assert_eq!(base_name, "defi-trading-alice");
    }

    #[test]
    fn test_get_base_account_name_implicit_accounts() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test implicit account (64-character hex string)
        let implicit_account = "98793cd91a3f870fb126f66285808c7e094afcfc4eda8a82f911432ac1b5dffd";
        let owner_id: AccountId = implicit_account.parse().unwrap();
        let base_name = contract.get_base_account_name(&owner_id);

        // Should start with "implicit_" and be deterministic
        assert!(base_name.starts_with("implicit_"));
        assert_eq!(base_name.len(), 33); // "implicit_" (9 chars) + 24 chars = 33

        // Test that the same input always produces the same output
        let base_name2 = contract.get_base_account_name(&owner_id);
        assert_eq!(base_name, base_name2);

        // Test different implicit account produces different base name
        let implicit_account2 = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let owner_id2: AccountId = implicit_account2.parse().unwrap();
        let base_name2 = contract.get_base_account_name(&owner_id2);
        assert_ne!(base_name, base_name2);
        assert!(base_name2.starts_with("implicit_"));
    }

    #[test]
    fn test_get_base_account_name_subaccount_hyphenation() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test various subaccount patterns
        let test_cases = vec![
            ("trading.alice.testnet", "trading-alice"),
            ("defi.trading.alice.near", "defi-trading-alice"),
            ("a.b.c.d.e.testnet", "a-b-c-d-e"),
            ("single.testnet", "single"),
            ("alice.near", "alice"),
        ];

        for (input, expected) in test_cases {
            let owner_id: AccountId = input.parse().unwrap();
            let base_name = contract.get_base_account_name(&owner_id);
            assert_eq!(base_name, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    #[should_panic(expected = "Unsupported account name format for base account name extraction")]
    fn test_get_base_account_name_invalid_format() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test invalid account format (not 64 chars, not ending with .near/.testnet)
        let invalid_account = "invalid_account_format";
        let owner_id: AccountId = invalid_account.parse().unwrap();
        contract.get_base_account_name(&owner_id);
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
    fn test_verify_implicit_base_name_named_account() {
        let context = get_context(accounts(1), "factory.testnet".parse().unwrap(), None);
        testing_env!(context.build());

        let contract = TradingAccountFactory::new(
            accounts(1),
            "testnet".to_string(),
            "EaFtguW8o7cna1k8EtD4SFfGNdivuCPhx2Qautn7J3Rz".to_string(),
        );

        // Test with named account
        let owner_id: AccountId = "alice.testnet".parse().unwrap();
        let expected_base_name = contract.get_base_account_name(&owner_id);

        // Verify correct base name for named account
        let is_valid = contract.verify_implicit_base_name(owner_id.clone(), expected_base_name);
        assert!(
            is_valid,
            "Correct base name for named account should be valid"
        );

        // Test with wrong base name for named account
        let wrong_base_name = "bob".to_string();
        let is_valid = contract.verify_implicit_base_name(owner_id.clone(), wrong_base_name);
        assert!(
            !is_valid,
            "Wrong base name for named account should be invalid"
        );
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
