/// Tests for pause/unpause emergency control.
#[cfg(test)]
mod test_pause {
    use crate::errors::QuickLendXError;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

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
