//! Two-step admin transfer and transfer lock end-to-end tests.

#[cfg(test)]
mod test_admin_two_step {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events},
        xdr, Address, Env, Symbol, TryFromVal,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    fn setup_with_admin() -> (Env, Address, Address) {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        initialize_admin(&env, &contract_id, &admin).unwrap();
        (env, contract_id, admin)
    }

    fn initialize_admin(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::initialize(env, admin))
    }

    /// Map a `try_*` client result back to the contract's own error type.
    fn call<E: core::fmt::Debug, I: core::fmt::Debug>(
        r: Result<Result<(), E>, Result<QuickLendXError, I>>,
    ) -> Result<(), QuickLendXError> {
        match r {
            Ok(Ok(())) => Ok(()),
            Err(Ok(e)) => Err(e),
            other => panic!("unexpected invocation result: {:?}", other),
        }
    }

    // State-changing operations go through the contract entrypoints so that
    // auth and events are recorded per invocation.
    fn transfer_admin(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        call(QuickLendXContractClient::new(env, contract_id).try_transfer_admin(current_admin, new_admin))
    }

    fn initiate_admin_transfer(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        call(
            QuickLendXContractClient::new(env, contract_id)
                .try_initiate_admin_transfer(current_admin, pending_admin),
        )
    }

    fn accept_admin_transfer(
        env: &Env,
        contract_id: &Address,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        call(QuickLendXContractClient::new(env, contract_id).try_accept_admin_transfer(pending_admin))
    }

    fn cancel_admin_transfer(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        call(QuickLendXContractClient::new(env, contract_id).try_cancel_admin_transfer(current_admin))
    }

    fn set_two_step_enabled(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        call(QuickLendXContractClient::new(env, contract_id).try_set_two_step_enabled(admin, &enabled))
    }

    fn get_admin(env: &Env, contract_id: &Address) -> Option<Address> {
        env.as_contract(contract_id, || AdminStorage::get_admin(env))
    }

    fn get_pending_admin(env: &Env, contract_id: &Address) -> Option<Address> {
        env.as_contract(contract_id, || AdminStorage::get_pending_admin(env))
    }

    fn is_admin(env: &Env, contract_id: &Address, address: &Address) -> bool {
        env.as_contract(contract_id, || AdminStorage::is_admin(env, address))
    }

    fn is_transfer_locked(env: &Env, contract_id: &Address) -> bool {
        env.as_contract(contract_id, || AdminStorage::is_transfer_locked(env))
    }

    fn is_two_step_enabled(env: &Env, contract_id: &Address) -> bool {
        env.as_contract(contract_id, || AdminStorage::is_two_step_enabled(env))
    }

    fn latest_topic_symbol(env: &Env) -> Symbol {
        let events = env.events().all();
        let last = events
            .events()
            .last()
            .expect("expected at least one emitted event");

        match &last.body {
            xdr::ContractEventBody::V0(body) => Symbol::try_from_val(
                env,
                body.topics
                    .first()
                    .expect("contract event should have at least one topic"),
            )
            .expect("first topic should be a Symbol"),
        }
    }

    /// Tests the complete happy path: initiate -> accept -> verify transfer.
    #[test]
    fn test_two_step_transfer_happy_path() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        // Enable two-step mode
        // `env.events().all()` holds only the most recent invocation's events
        // (reads through `as_contract` count too), so check topics first.
        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_2st"));
        assert!(is_two_step_enabled(&env, &contract_id));

        // Initiate transfer
        initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_2).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_req"));
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2.clone()));
        assert!(is_transfer_locked(&env, &contract_id));

        // Accept transfer
        accept_admin_transfer(&env, &contract_id, &admin_2).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_trf"));

        // Verify state changes
        assert_eq!(get_admin(&env, &contract_id), Some(admin_2.clone()));
        assert!(is_admin(&env, &contract_id, &admin_2));
        assert!(!is_admin(&env, &contract_id, &admin_1));
        assert_eq!(get_pending_admin(&env, &contract_id), None);
        assert!(!is_transfer_locked(&env, &contract_id));
    }

    /// Tests the cancel path: initiate -> cancel -> verify state reverted.
    #[test]
    fn test_two_step_transfer_cancel_path() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_2).unwrap();

        // Cancel the pending transfer
        cancel_admin_transfer(&env, &contract_id, &admin_1).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_cnl"));

        // Verify state is reverted
        assert_eq!(get_admin(&env, &contract_id), Some(admin_1.clone()));
        assert_eq!(get_pending_admin(&env, &contract_id), None);
        assert!(!is_transfer_locked(&env, &contract_id));
    }

    /// Tests negative path: a third party cannot accept the pending transfer.
    #[test]
    fn test_unauthorized_acceptance_rejected() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let third_party = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_2).unwrap();

        // Third party accepts
        assert_eq!(
            accept_admin_transfer(&env, &contract_id, &third_party),
            Err(QuickLendXError::Unauthorized)
        );
        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2));
    }

    /// Tests negative path: accepting when there is no pending transfer fails.
    #[test]
    fn test_accept_without_pending_fails() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();

        // Accept with no pending transfer
        assert_eq!(
            accept_admin_transfer(&env, &contract_id, &admin_2),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    /// Tests negative path: initiating while transfer lock is active or pending transfer exists fails.
    #[test]
    fn test_initiate_while_locked_or_pending_fails() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let admin_3 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_2).unwrap();

        // Attempt to initiate a second transfer
        assert_eq!(
            initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_3),
            Err(QuickLendXError::OperationNotAllowed)
        );
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2));
    }

    /// Tests negative path: initiating self-transfer (pending admin == current admin) fails.
    #[test]
    fn test_initiate_self_transfer_fails() {
        let (env, contract_id, admin_1) = setup_with_admin();

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();

        assert_eq!(
            initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_1),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    /// Tests direct transfer is blocked when a two-step transfer is pending/locked.
    #[test]
    fn test_direct_transfer_blocked_during_pending_flow() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let admin_3 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_2).unwrap();

        // Attempt direct transfer
        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &admin_3),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }
}
