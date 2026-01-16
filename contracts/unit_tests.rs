#[cfg(test)]
mod tests {
    use crate::{
        ActionString, BigR, EcdsaSignatureResponse, ScalarValue, SignatureResponse,
        TradingAccountContract,
    };
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
    fn test_new() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = TradingAccountContract::new(
            accounts(1),
            AccountId::try_from("v1.signer-prod.testnet".to_string()).unwrap(),
        );
        assert_eq!(contract.get_owner_id(), accounts(1));
    }

    #[test]
    fn test_authorize_user() {
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
    fn test_unauthorized_add_user() {
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
    fn test_max_authorized_users_limit() {
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
    fn test_max_authorized_users_remove_and_add() {
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
    fn test_unauthorized_request_signature() {
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
    fn test_disallowed_action() {
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
    fn test_request_signature_single_transfer_action_fails() {
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
    fn test_request_signature_multiple_actions_with_transfer_succeeds() {
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
    fn test_request_signature_multiple_transfer_actions_without_function_call_fails() {
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
        let decoded: SignatureResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.scheme, "Secp256k1");
        assert_eq!(decoded.recovery_id, 1);
    }

    #[test]
    fn test_add_full_access_key_owner() {
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
    fn test_add_full_access_key_non_owner() {
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
    fn test_add_full_access_key_and_register_with_intents_owner() {
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
    fn test_add_full_access_key_and_register_with_intents_non_owner() {
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
    fn test_validate_and_build_actions_valid_function_call() {
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
    fn test_validate_and_build_actions_valid_transfer() {
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
    fn test_validate_and_build_actions_disallowed_contract() {
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
    fn test_validate_and_build_actions_disallowed_method() {
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
    fn test_validate_and_build_actions_invalid_gas_format() {
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
    fn test_validate_and_build_actions_invalid_deposit_format() {
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
    fn test_validate_and_build_actions_multiple_actions() {
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
    fn test_validate_and_build_actions_empty_actions() {
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
    fn test_create_signature_request_with_domain_id() {
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
    fn test_create_signature_request_without_domain_id() {
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
    fn test_set_signer_id_owner() {
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
    fn test_set_signer_id_authorized_user() {
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
    fn test_set_signer_id_unauthorized() {
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
}
