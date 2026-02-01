/// Tests for pause/unpause emergency control.
#[cfg(test)]
mod test_pause {
    use crate::errors::QuickLendXError;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use invoice::InvoiceCategory;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

    const TEST_BYTES: [u8; 32] = [0u8; 32];

    fn setup() -> (Env, QuickLendXContractClient<'static>) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        (env, client)
    }

    #[test]
    fn test_pause_toggle_and_queries() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let _ = client.try_initialize_admin(&admin);

        assert!(!client.is_paused(), "protocol should start unpaused");

        client.pause(&admin);
        assert!(client.is_paused(), "pause should toggle the flag");

        // Read-only query should still be available while paused.
        assert_eq!(client.get_current_admin(), Some(admin.clone()));

        client.unpause(&admin);
        assert!(!client.is_paused(), "unpause should clear the flag");
    }

    #[test]
    fn test_pause_blocks_mutating_entrypoints() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let currency = Address::generate(&env);
        let _ = client.try_initialize_admin(&admin);

        client.pause(&admin);

        let result = client.try_add_currency(&admin, &currency);
        assert!(result.is_err());
        let err = result.err().unwrap();
        let contract_err = err.expect("expected contract error");
        assert_eq!(contract_err, QuickLendXError::ProtocolPaused);

        client.unpause(&admin);

        let result = client.try_add_currency(&admin, &currency);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mutations_blocked_and_reads_ok_during_pause() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let _ = client.try_initialize_admin(&admin);

        let business = Address::generate(&env);
        let currency = Address::generate(&env);
        let timestamp = env.ledger().timestamp();

        let invoice_id = client
            .store_invoice(
                &business,
                &1_000,
                &currency,
                &(timestamp + 86_400),
                &String::from_str(&env, "Pause invoice"),
                &InvoiceCategory::Services,
                &Vec::new(&env),
            )
            .expect("should store invoice");

        client.pause(&admin);

        let upload_err = client.try_upload_invoice(
            &business,
            &1_000,
            &currency,
            &(timestamp + 86_400),
            &String::from_str(&env, "Pause upload"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
        assert_eq!(
            upload_err
                .unwrap_err()
                .expect("expected contract error"),
            QuickLendXError::ProtocolPaused
        );

        let bid_err =
            client.try_place_bid(&Address::generate(&env), &invoice_id, &500, &650);
        assert_eq!(
            bid_err.unwrap_err().expect("expected contract error"),
            QuickLendXError::ProtocolPaused
        );

        let accept_err = client.try_accept_bid(&invoice_id, &BytesN::from_array(&env, &TEST_BYTES));
        assert_eq!(
            accept_err.unwrap_err().expect("expected contract error"),
            QuickLendXError::ProtocolPaused
        );

        let settle_err = client.try_settle_invoice(&invoice_id, &1_000);
        assert_eq!(
            settle_err.unwrap_err().expect("expected contract error"),
            QuickLendXError::ProtocolPaused
        );

        let loaded = client.get_invoice(invoice_id.clone());
        assert!(loaded.is_ok(), "read-only query should still succeed");
        assert_eq!(loaded.unwrap().id, invoice_id);

        let non_admin = Address::generate(&env);
        let unpause_err = client.try_unpause(&non_admin);
        assert_eq!(
            unpause_err.unwrap_err().expect("expected contract error"),
            QuickLendXError::NotAdmin
        );

        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_only_admin_can_pause() {
        let (env, client) = setup();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        let _ = client.try_initialize_admin(&admin);

        let result = client.try_pause(&non_admin);
        assert!(result.is_err());
        let err = result.err().unwrap();
        let contract_err = err.expect("expected contract error");
        assert_eq!(contract_err, QuickLendXError::NotAdmin);
    }
}
