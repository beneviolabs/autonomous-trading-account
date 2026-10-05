#[cfg(test)]
mod tests {
    use crate::{ActionString, BigR, EcdsaSignatureResponse, ScalarValue, TradingAccountContract};
    use near_sdk::PublicKey;
    use near_sdk::{
        AccountId,
        json_types::{Base58CryptoHash, U64},
        test_utils::{VMContextBuilder, accounts},
        testing_env,
    };
    use omni_transaction::TxBuilder;
    use omni_transaction::near::utils::PublicKeyStrExt;
    use std::str::FromStr;

    fn get_context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor)
            .prepaid_gas(near_sdk::Gas::from_tgas(150));
        builder
    }

    #[test]
    fn test_new_sets_owner() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        assert_eq!(contract.get_owner_id(), accounts(1));
    }

    #[test]
    fn test_add_authorized_user() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        contract.add_authorized_user(accounts(2));
        assert!(contract.is_authorized(accounts(2)));
    }

    #[test]
    fn test_remove_authorized_user() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer".to_string()).unwrap(),
        );

        contract.add_authorized_user(accounts(2));
        assert!(contract.is_authorized(accounts(2)));

        contract.remove_authorized_user(accounts(2));
        assert!(!contract.is_authorized(accounts(2)));
    }

    #[test]
    #[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
    fn test_add_authorized_user_rejects_non_owner() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        contract.add_authorized_user(accounts(3));
    }

    #[test]
    fn test_get_authorized_users() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        contract.add_authorized_user(accounts(2));
        contract.add_authorized_user(accounts(3));

        let users = contract.get_authorized_users();
        assert_eq!(users.len(), 2);
        assert!(users.contains(&accounts(2)));
        assert!(users.contains(&accounts(3)));
    }

    #[test]
    #[should_panic(expected = "Maximum number of authorized users reached:(10)")]
    fn test_add_authorized_user_rejects_more_than_max() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        // Add 10 authorized users (the maximum)
        for i in 0..10 {
            let user = AccountId::try_from(format!("user{}.testnet", i)).unwrap();
            contract.add_authorized_user(user);
        }

        // Attempting to add an 11th user should panic
        let user11 = AccountId::try_from("user11.testnet".to_string()).unwrap();
        contract.add_authorized_user(user11);
    }

    #[test]
    fn test_remove_authorized_user_frees_a_slot() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        // Add 10 authorized users (the maximum)
        for i in 0..10 {
            let user = AccountId::try_from(format!("user{}.testnet", i)).unwrap();
            contract.add_authorized_user(user);
        }

        // Verify we have 10 users
        assert_eq!(contract.get_authorized_users().len(), 10);

        // Remove one user
        let user0 = AccountId::try_from("user0.testnet".to_string()).unwrap();
        contract.remove_authorized_user(user0);
        assert_eq!(contract.get_authorized_users().len(), 9);

        // Now we should be able to add another user
        let user11 = AccountId::try_from("user11.testnet".to_string()).unwrap();
        contract.add_authorized_user(user11.clone());
        assert_eq!(contract.get_authorized_users().len(), 10);
        assert!(contract.is_authorized(user11));
    }

    #[test]
    #[should_panic(expected = "Unauthorized: only authorized users can request signatures")]
    fn test_request_signature_rejects_unauthorized_caller() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        let _ = contract.request_signature(
            accounts(3),                                        // contract_id: AccountId
            "[{\"public_key\": \"ed25519:1234\"}]".to_string(), // actions_json: String
            U64(1),                                             // nonce: U64
            Base58CryptoHash::from([0u8; 32]),                  // block_hash: Base58CryptoHash
            "secp256k1:abcd".to_string(),                       // public_key: String
            "test_path".to_string(),                            // path: String
            None,                                               // domain_id: Option<u32>
        );
    }

    #[test]
    #[should_panic(
        expected = "unknown variant `Sign Message`, expected `FunctionCall` or `Transfer`"
    )]
    fn test_request_signature_rejects_unknown_action_type() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        testing_env!(get_context(accounts(1)).build());
        contract.add_authorized_user(accounts(2));

        let actions_json = r#"[
            {
                "type": "Sign Message",
                "Message": "blah blah blah"
            }
        ]"#;

        testing_env!(get_context(accounts(2)).build());
        let _ = contract.request_signature(
            accounts(3),                       // contract_id
            actions_json.to_string(),          // actions_json
            U64(1),                            // nonce
            Base58CryptoHash::from([0u8; 32]), // block_hash
            "secp256k1:abcd".to_string(),      // public_key
            "ed25519:wxyz".to_string(),        // path
            None,                              // domain_id: Option<u32>
        );
    }

    #[test]
    #[should_panic(
        expected = "Transfer actions must be accompanied by at least one FunctionCall action"
    )]
    fn test_request_signature_rejects_lone_transfer() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        testing_env!(get_context(accounts(1)).build());
        contract.add_authorized_user(accounts(2));

        let actions_json = r#"[
            {
                "type": "Transfer",
                "deposit": "1000000000000000000000000"
            }
        ]"#;

        testing_env!(get_context(accounts(2)).build());
        let _ = contract.request_signature(
            AccountId::try_from("bad-account.near".to_string()).unwrap(),
            actions_json.to_string(),
            U64(1),
            Base58CryptoHash::from([0u8; 32]),
            "ed25519:11111111111111111111111111111111".to_string(),
            "trading-account.near".to_string(),
            None, // domain_id: Option<u32>
        );
    }

    #[test]
    #[should_panic(expected = "GasExceeded")]
    fn test_request_signature_accepts_transfer_with_function_call() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        testing_env!(get_context(accounts(1)).build());
        contract.add_authorized_user(accounts(2));

        let actions_json = r#"[
            {
                "type": "FunctionCall",
                "method_name": "ft_transfer_call",
                "args": {"receiver_id": "alice.near", "amount": "1000000000000000000000000"},
                "gas": "100000000000000",
                "deposit": "1000000000000000000000000"
            },
            {
                "type": "Transfer",
                "deposit": "1000000000000000000000000"
            }
        ]"#;

        testing_env!(get_context(accounts(2)).build());
        let _result = contract.request_signature(
            AccountId::try_from("wrap.near".to_string()).unwrap(),
            actions_json.to_string(),
            U64(1),
            Base58CryptoHash::from([0u8; 32]),
            "ed25519:11111111111111111111111111111111".to_string(),
            "trading-account.near".to_string(),
            None, // domain_id: Option<u32>
        );
        // Test passes  - gas exceeded - but validation succeeds
    }

    #[test]
    #[should_panic(
        expected = "Transfer actions must be accompanied by at least one FunctionCall action"
    )]
    fn test_request_signature_rejects_transfers_without_function_call() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        testing_env!(get_context(accounts(1)).build());
        contract.add_authorized_user(accounts(2));

        let actions_json = r#"[
            {
                "type": "Transfer",
                "deposit": "1000000000000000000000000"
            },
            {
                "type": "Transfer",
                "deposit": "2000000000000000000000000"
            }
        ]"#;

        testing_env!(get_context(accounts(2)).build());
        let _ = contract.request_signature(
            AccountId::try_from("wrap.near".to_string()).unwrap(),
            actions_json.to_string(),
            U64(1),
            Base58CryptoHash::from([0u8; 32]),
            "ed25519:11111111111111111111111111111111".to_string(),
            "trading-account.near".to_string(),
            None, // domain_id: Option<u32>
        );
    }

    #[test]
    fn test_signature_response_serialization() {
        // Test ECDSA signature response
        let ecdsa_response = EcdsaSignatureResponse {
            scheme: "Secp256k1".to_string(),
            big_r: BigR {
                affine_point: "03D0E412BEEBF4B0191C08E13323466A96582C95A2B0BAF4CB6859968B86C01157"
                    .to_string(),
            },
            s: ScalarValue {
                scalar: "1AE54A1E7D404FD655B43C05DA78D1A6DC5ABAC2AE2A8338F03580D14A2C17F9"
                    .to_string(),
            },
            recovery_id: 1,
        };

        let json = serde_json::to_string(&ecdsa_response).unwrap();
        let decoded: EcdsaSignatureResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.scheme, "Secp256k1");
        assert_eq!(decoded.recovery_id, 1);
    }

    #[test]
    fn test_add_full_access_key_by_owner() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        let pk = PublicKey::from_str("ed25519:11111111111111111111111111111111").unwrap();
        let result = contract.add_full_access_key(pk);
        // We can't fully test Promise chain, but we can check the type
        assert!(matches!(result, near_sdk::Promise { .. }));
    }

    #[test]
    #[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
    fn test_add_full_access_key_rejects_non_owner() {
        let context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        let pk = PublicKey::from_str("ed25519:11111111111111111111111111111111").unwrap();
        let _ = contract.add_full_access_key(pk);
    }

    #[test]
    fn test_add_full_access_key_and_register_with_intents_by_owner() {
        let mut context = get_context(accounts(1));
        context.attached_deposit(near_sdk::NearToken::from_yoctonear(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        let pk = PublicKey::from_str("ed25519:11111111111111111111111111111111").unwrap();
        let result = contract.add_full_access_key_and_register_with_intents(pk);
        assert!(matches!(result, near_sdk::Promise { .. }));
    }

    #[test]
    #[should_panic(expected = "You have no power here. Only the owner can perform this action.")]
    fn test_add_full_access_key_and_register_with_intents_rejects_non_owner() {
        let mut context = get_context(accounts(2));
        context.attached_deposit(near_sdk::NearToken::from_yoctonear(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        let pk = PublicKey::from_str("ed25519:11111111111111111111111111111111").unwrap();
        let _ = contract.add_full_access_key_and_register_with_intents(pk);
    }

    #[test]
    fn test_validate_and_build_actions_accepts_function_call() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer.testnet".to_string()).unwrap(),
        );

        let actions = vec![ActionString::FunctionCall {
            method_name: "ft_transfer_call".to_string(),
            args: serde_json::json!({"receiver_id": "alice.near", "amount": "1000000000000000000000000"}),
            gas: "100000000000000".to_string(),
            deposit: "1000000000000000000000000".to_string(),
        }];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_ok());
        let omni_actions = result.unwrap();
        assert_eq!(omni_actions.len(), 1);
    }

    #[test]
    fn test_validate_and_build_actions_accepts_transfer_with_function_call() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let actions = vec![
            ActionString::Transfer {
                deposit: "500000000000000000000000".to_string(),
            },
            ActionString::FunctionCall {
                method_name: "ft_transfer_call".to_string(),
                args: serde_json::json!({"receiver_id": "alice.near", "amount": "1000000000000000000000000"}),
                gas: "100000000000000".to_string(),
                deposit: "1000000000000000000000000".to_string(),
            },
        ];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_ok());
        let omni_actions = result.unwrap();
        assert_eq!(omni_actions.len(), 2);
    }

    #[test]
    fn test_validate_and_build_actions_rejects_contract_outside_allowlist() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let actions = vec![ActionString::FunctionCall {
            method_name: "ft_transfer_call".to_string(),
            args: serde_json::json!({}),
            gas: "100000000000000".to_string(),
            deposit: "1000000000000000000000000".to_string(),
        }];

        let contract_id = AccountId::try_from("disallowed.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("is not allowed"));
    }

    #[test]
    fn test_validate_and_build_actions_rejects_method_outside_allowlist() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let actions = vec![ActionString::FunctionCall {
            method_name: "disallowed_method".to_string(),
            args: serde_json::json!({}),
            gas: "100000000000000".to_string(),
            deposit: "1000000000000000000000000".to_string(),
        }];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Method disallowed_method is restricted"));
    }

    #[test]
    fn test_validate_and_build_actions_rejects_invalid_gas() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let actions = vec![ActionString::FunctionCall {
            method_name: "ft_transfer_call".to_string(),
            args: serde_json::json!({}),
            gas: "invalid_gas".to_string(),
            deposit: "1000000000000000000000000".to_string(),
        }];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Invalid gas format"));
    }

    #[test]
    fn test_validate_and_build_actions_rejects_invalid_deposit() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let actions = vec![ActionString::FunctionCall {
            method_name: "ft_transfer_call".to_string(),
            args: serde_json::json!({}),
            gas: "100000000000000".to_string(),
            deposit: "invalid_deposit".to_string(),
        }];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Invalid deposit format"));
    }

    #[test]
    fn test_validate_and_build_actions_accepts_multiple_actions() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let actions = vec![
            ActionString::FunctionCall {
                method_name: "ft_transfer_call".to_string(),
                args: serde_json::json!({"receiver_id": "alice.near"}),
                gas: "100000000000000".to_string(),
                deposit: "1000000000000000000000000".to_string(),
            },
            ActionString::Transfer {
                deposit: "500000000000000000000000".to_string(),
            },
            ActionString::FunctionCall {
                method_name: "near_deposit".to_string(),
                args: serde_json::json!({}),
                gas: "50000000000000".to_string(),
                deposit: "0".to_string(),
            },
        ];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_ok());
        let omni_actions = result.unwrap();
        assert_eq!(omni_actions.len(), 3);
    }

    #[test]
    fn test_validate_and_build_actions_rejects_empty_actions() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer.testnet".to_string()).unwrap(),
        );

        let actions = vec![];

        let contract_id = AccountId::try_from("wrap.near".to_string()).unwrap();
        let result = contract.validate_and_build_actions(actions, &contract_id);

        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Actions cannot be empty"));
    }

    #[test]
    fn test_create_signature_request_passes_domain_id() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer.testnet".to_string()).unwrap(),
        );

        // Create a simple mock transaction using the same pattern as in the main code
        let tx = omni_transaction::TransactionBuilder::new::<omni_transaction::NEAR>()
            .signer_id("test.near".to_string())
            .signer_public_key(
                "ed25519:11111111111111111111111111111111"
                    .to_public_key()
                    .unwrap(),
            )
            .nonce(1)
            .receiver_id("wrap.near".to_string())
            .block_hash(omni_transaction::near::types::BlockHash([0u8; 32]))
            .actions(vec![])
            .build();

        let result = contract.create_signature_request(
            &tx,
            "test.trading-account.near".to_string(),
            Some(1),
        );

        // Verify the result is valid JSON
        assert!(result.is_object());

        // Verify the structure contains the expected fields
        let request_obj = result.get("request").unwrap();
        assert!(request_obj.get("payload_v2").is_some());
        assert!(request_obj.get("path").is_some());
        assert!(request_obj.get("domain_id").is_some());

        // Verify specific values
        assert_eq!(
            request_obj.get("path").unwrap().as_str().unwrap(),
            "test.trading-account.near"
        );
        assert_eq!(request_obj.get("domain_id").unwrap(), 1);
    }

    #[test]
    fn test_create_signature_request_defaults_domain_id_to_zero() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer.testnet".to_string()).unwrap(),
        );

        // Create a simple mock transaction using the same pattern as in the main code
        let tx = omni_transaction::TransactionBuilder::new::<omni_transaction::NEAR>()
            .signer_id("test.near".to_string())
            .signer_public_key(
                "ed25519:11111111111111111111111111111111"
                    .to_public_key()
                    .unwrap(),
            )
            .nonce(1)
            .receiver_id("wrap.near".to_string())
            .block_hash(omni_transaction::near::types::BlockHash([0u8; 32]))
            .actions(vec![])
            .build();

        let result =
            contract.create_signature_request(&tx, "test.trading-account.near".to_string(), None);

        // Verify the result is valid JSON
        assert!(result.is_object());

        // Verify the structure contains the expected fields
        let request_obj = result.get("request").unwrap();
        assert!(request_obj.get("payload_v2").is_some());
        assert!(request_obj.get("path").is_some());
        assert!(request_obj.get("domain_id").is_some());

        // Verify specific values
        assert_eq!(
            request_obj.get("path").unwrap().as_str().unwrap(),
            "test.trading-account.near"
        );
        assert_eq!(request_obj.get("domain_id").unwrap().as_u64().unwrap(), 0); // Should default to NEAR_MPC_DOMAIN_ID
    }

    #[test]
    fn test_convert_deposits_to_strings_small_numbers() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let json_input = r#"{"deposit":1000000,"other_field":"value"}"#.to_string();

        let result = contract.convert_deposits_to_strings(
            json_input,
            &[omni_transaction::near::types::U128(1000000)],
        );

        // All deposit values should be converted to strings
        assert_eq!(result, r#"{"deposit":"1000000","other_field":"value"}"#);
    }

    #[test]
    fn test_convert_deposits_to_strings_large_numbers() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let large_number = 10_000_000_000_000_000_000_000u128; // Larger than MAX_SAFE_INTEGER
        let json_input = format!(r#"{{"deposit":{},"other_field":"value"}}"#, large_number);

        let result = contract.convert_deposits_to_strings(
            json_input,
            &[omni_transaction::near::types::U128(large_number)],
        );

        // Large numbers should be converted to strings (no scientific notation)
        assert_eq!(
            result,
            format!(r#"{{"deposit":"{}","other_field":"value"}}"#, large_number)
        );
    }

    #[test]
    fn test_convert_deposits_to_strings_multiple_deposits() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let large_number1 = 10_000_000_000_000_000_000_000u128;
        let large_number2 = 20_000_000_000_000_000_000_000u128;
        let small_number = 1000000u128;

        let json_input = format!(
            r#"{{"actions":[{{"deposit":{}}},{{"deposit":{}}},{{"deposit":{}}}]}}"#,
            large_number1, small_number, large_number2
        );

        let result = contract.convert_deposits_to_strings(
            json_input,
            &[
                omni_transaction::near::types::U128(large_number1),
                omni_transaction::near::types::U128(small_number),
                omni_transaction::near::types::U128(large_number2),
            ],
        );

        // All deposit values should be converted to strings (no scientific notation)
        let expected = format!(
            r#"{{"actions":[{{"deposit":"{}"}},{{"deposit":"{}"}},{{"deposit":"{}"}}]}}"#,
            large_number1, small_number, large_number2
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_set_signer_id_by_owner() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        let new_signer_id = AccountId::try_from("new-signer.near".to_string()).unwrap();
        contract.set_signer_id(new_signer_id.clone());

        assert_eq!(contract.get_signer_id(), new_signer_id);
    }

    #[test]
    fn test_set_signer_id_by_authorized_user() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        // Add accounts(2) as authorized user
        contract.add_authorized_user(accounts(2));

        // Switch to accounts(2) context
        let context = get_context(accounts(2));
        testing_env!(context.build());

        let new_signer_id = AccountId::try_from("new-signer.near".to_string()).unwrap();
        contract.set_signer_id(new_signer_id.clone());

        assert_eq!(contract.get_signer_id(), new_signer_id);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: only authorized users can set signer ID")]
    fn test_set_signer_id_rejects_unauthorized_caller() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );

        // Switch to unauthorized user (accounts(3))
        let context = get_context(accounts(3));
        testing_env!(context.build());

        let new_signer_id = AccountId::try_from("new-signer.near".to_string()).unwrap();
        contract.set_signer_id(new_signer_id);
    }

    // Pins the bytes that get signed and broadcast. request_signature builds the transaction with
    // omni-transaction and passes it to sign_request_callback as JSON, which rebuilds it as
    // models::NearTransaction. The expected values were produced by the audited version
    // (omni-transaction 0.2, near-sdk 5.17), so a dependency bump that changes the encoding or
    // breaks the JSON round trip fails here.
    #[test]
    fn test_transaction_bytes_unchanged() {
        use crate::{models, test_support};
        use omni_transaction::near::types::{BlockHash, Secp256K1Signature, Signature};
        use omni_transaction::{NEAR, TransactionBuilder};

        let mut context = get_context(accounts(1));
        context.current_account_id(
            "implicit_07454f3217b9229ead97798c.auth.peerfolio.near"
                .parse()
                .unwrap(),
        );
        testing_env!(context.build());
        let contract = TradingAccountContract::new(accounts(1), "v1.signer".parse().unwrap());

        let actions: Vec<ActionString> = serde_json::from_str(
            r#"[
                {"type":"FunctionCall","method_name":"near_deposit","args":{"a":[1,2,"x"]},"gas":"30000000000000","deposit":"123456789012345678901234567"},
                {"type":"FunctionCall","method_name":"ft_transfer_call","args":{},"gas":"300000000000000","deposit":"1"},
                {"type":"Transfer","deposit":"340282366920938463463374607431768211455"}
            ]"#,
        )
        .unwrap();
        let contract_id: AccountId = "wrap.near".parse().unwrap();
        let omni_actions = contract
            .validate_and_build_actions(actions, &contract_id)
            .unwrap();
        let tx = TransactionBuilder::new::<NEAR>()
            .signer_id(near_sdk::env::current_account_id().to_string())
            .signer_public_key(
                "secp256k1:3tFRbMqmoa6AAALMrEFAYCEoHcqKxeW38YptwowBVBtXK1vo36HDbUWuR6EZmoK4JcH6HDkNMGGqP1ouV7VZUWya"
                    .to_public_key()
                    .unwrap(),
            )
            .nonce(42)
            .receiver_id(contract_id.to_string())
            .block_hash(BlockHash([7u8; 32]))
            .actions(omni_actions.clone())
            .build();

        let expected_for_signing = "35000000696d706c696369745f3037343534663332313762393232396561643937373938632e617574682e70656572666f6c696f2e6e65617201903a9a9933ed92bdda3fcf30ac999060a5a0fa51c2b6c74838d3029a5aadefe038f7e4a91714f42bb5a2459a0d294be0cd047b4a999d6fd912702470f843271d2a0000000000000009000000777261702e6e656172070707070707070707070707070707070707070707070707070707070707070703000000020c0000006e6561725f6465706f7369740f0000007b2261223a5b312c322c2278225d7d00e057eb481b0000874b9f2ca8f258f1fd1e660000000000021000000066745f7472616e736665725f63616c6c020000007b7d00c06e31d91001000100000000000000000000000000000003ffffffffffffffffffffffffffffffff";
        assert_eq!(hex::encode(tx.build_for_signing()), expected_for_signing);

        // Same round trip as request_signature -> sign_request_callback.
        let tx_json = test_support::callback_json(&contract, &tx, &omni_actions);
        let near_tx: models::NearTransaction = serde_json::from_str(&tx_json).unwrap();
        assert_eq!(
            hex::encode(near_tx.build_for_signing()),
            expected_for_signing
        );

        let mut signature = [0u8; 65];
        for (i, byte) in signature.iter_mut().enumerate() {
            *byte = i as u8;
        }
        assert_eq!(
            hex::encode(
                near_tx.build_with_signature(Signature::SECP256K1(Secp256K1Signature(signature)))
            ),
            "35000000696d706c696369745f3037343534663332313762393232396561643937373938632e617574682e70656572666f6c696f2e6e65617201903a9a9933ed92bdda3fcf30ac999060a5a0fa51c2b6c74838d3029a5aadefe038f7e4a91714f42bb5a2459a0d294be0cd047b4a999d6fd912702470f843271d2a0000000000000009000000777261702e6e656172070707070707070707070707070707070707070707070707070707070707070703000000020c0000006e6561725f6465706f7369740f0000007b2261223a5b312c322c2278225d7d00e057eb481b0000874b9f2ca8f258f1fd1e660000000000021000000066745f7472616e736665725f63616c6c020000007b7d00c06e31d91001000100000000000000000000000000000003ffffffffffffffffffffffffffffffff01000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40"
        );
    }

    // sign_request_callback with a response shaped like v1.signer's: it must parse the response,
    // rebuild the transaction from request_signature's JSON and attach the signature.
    fn callback_setup() -> (
        TradingAccountContract,
        omni_transaction::near::NearTransaction,
        String,
    ) {
        use crate::test_support;

        let mut context = get_context(accounts(1));
        context.current_account_id(
            "implicit_07454f3217b9229ead97798c.auth.peerfolio.near"
                .parse()
                .unwrap(),
        );
        testing_env!(context.build());
        let contract = TradingAccountContract::new(accounts(1), "v1.signer".parse().unwrap());
        let (tx, tx_json) = test_support::unsigned_tx(
            &contract,
            "wrap.near",
            r#"[{"type":"FunctionCall","method_name":"near_deposit","args":{},"gas":"30000000000000","deposit":"50000000000000000000000"},{"type":"Transfer","deposit":"1"}]"#,
            7,
            [9u8; 32],
        );
        (contract, tx, tx_json)
    }

    #[test]
    fn test_sign_request_callback_attaches_mpc_signature() {
        let (mut contract, tx, tx_json) = callback_setup();
        let (response, expected_signed) = crate::test_support::mpc_sign(&tx);

        let signed_base64 = contract.sign_request_callback(Ok(response), tx_json);

        let signed = near_sdk::base64::Engine::decode(
            &near_sdk::base64::engine::general_purpose::STANDARD,
            signed_base64,
        )
        .unwrap();
        assert_eq!(signed, expected_signed);
    }

    #[test]
    #[should_panic(expected = "Failed to parse the MPC's Signature response")]
    fn test_sign_request_callback_rejects_signer_failure() {
        let (mut contract, _, tx_json) = callback_setup();
        contract.sign_request_callback(Err(near_sdk::PromiseError::Failed), tx_json);
    }
}
