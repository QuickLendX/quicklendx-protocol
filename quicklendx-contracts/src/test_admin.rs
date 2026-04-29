//! Admin transfer safety tests.
//!
//! Coverage goals for issue #824:
//! - one-time initialization
//! - authenticated transfer
//! - transfer lock behavior
//! - optional two-step transfer flow
//! - event emission on admin-state changes

#[cfg(test)]
mod test_admin {
    use crate::admin::{AdminStorage, ADMIN_INITIALIZED_KEY};
    use crate::errors::QuickLendXError;
    use crate::QuickLendXContract;
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

    fn transfer_admin(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::transfer_admin(env, current_admin, new_admin))
    }

    fn initiate_admin_transfer(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::initiate_admin_transfer(env, current_admin, pending_admin)
        })
    }

    fn accept_admin_transfer(
        env: &Env,
        contract_id: &Address,
        pending_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::accept_admin_transfer(env, pending_admin))
    }

    fn cancel_admin_transfer(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::cancel_admin_transfer(env, current_admin))
    }

    fn set_two_step_enabled(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::set_two_step_enabled(env, admin, enabled))
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

    fn is_initialized(env: &Env, contract_id: &Address) -> bool {
        env.as_contract(contract_id, || AdminStorage::is_initialized(env))
    }

    fn is_transfer_locked(env: &Env, contract_id: &Address) -> bool {
        env.as_contract(contract_id, || AdminStorage::is_transfer_locked(env))
    }

    fn is_two_step_enabled(env: &Env, contract_id: &Address) -> bool {
        env.as_contract(contract_id, || AdminStorage::is_two_step_enabled(env))
    }

    fn set_admin_alias(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
        new_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::set_admin(env, current_admin, new_admin))
    }

    fn require_admin_auth(
        env: &Env,
        contract_id: &Address,
        address: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::require_admin_auth(env, address))
    }

    fn require_current_admin(
        env: &Env,
        contract_id: &Address,
    ) -> Result<Address, QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::require_current_admin(env))
    }

    fn set_admin_legacy(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::set_admin_legacy(env, admin))
    }

    fn with_admin_auth_ok(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
    ) -> Result<u32, QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::with_admin_auth(env, admin, || Ok(42)))
    }

    fn with_admin_auth_err(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
    ) -> Result<u32, QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::with_admin_auth(env, admin, || Err(QuickLendXError::OperationNotAllowed))
        })
    }

    fn with_current_admin_ok(
        env: &Env,
        contract_id: &Address,
    ) -> Result<Address, QuickLendXError> {
        env.as_contract(contract_id, || AdminStorage::with_current_admin(env, |admin| Ok(admin.clone())))
    }

    fn with_current_admin_err(
        env: &Env,
        contract_id: &Address,
    ) -> Result<Address, QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::with_current_admin(env, |_admin| Err(QuickLendXError::OperationNotAllowed))
        })
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

    #[test]
    fn initialize_is_one_time_only() {
        let (env, contract_id) = setup();
        let admin_1 = Address::generate(&env);
        let admin_2 = Address::generate(&env);

        assert_eq!(initialize_admin(&env, &contract_id, &admin_1), Ok(()));
        assert_eq!(
            initialize_admin(&env, &contract_id, &admin_2),
            Err(QuickLendXError::OperationNotAllowed)
        );

        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
        assert!(is_initialized(&env, &contract_id));
    }

    #[test]
    fn transfer_before_initialization_is_rejected() {
        let (env, contract_id) = setup();
        let admin_1 = Address::generate(&env);
        let admin_2 = Address::generate(&env);

        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &admin_2),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn transfer_requires_current_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let attacker = Address::generate(&env);
        let replacement = Address::generate(&env);

        assert_eq!(
            transfer_admin(&env, &contract_id, &attacker, &replacement),
            Err(QuickLendXError::NotAdmin)
        );
        assert_eq!(get_admin(&env, &contract_id), Some(admin));
    }

    #[test]
    fn transfer_rejects_self_transfer() {
        let (env, contract_id, admin) = setup_with_admin();
        assert_eq!(
            transfer_admin(&env, &contract_id, &admin, &admin),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn direct_transfer_updates_admin_atomically() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &admin_2),
            Ok(())
        );

        assert_eq!(get_admin(&env, &contract_id), Some(admin_2.clone()));
        assert!(is_admin(&env, &contract_id, &admin_2));
        assert!(!is_transfer_locked(&env, &contract_id));
        assert_eq!(get_pending_admin(&env, &contract_id), None);
    }

    #[test]
    fn two_step_transfer_sets_pending_and_lock() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();

        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2));
        assert!(is_transfer_locked(&env, &contract_id));
    }

    #[test]
    fn direct_initiate_transfer_api_is_enforced_and_works() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let attacker = Address::generate(&env);

        assert_eq!(
            initiate_admin_transfer(&env, &contract_id, &attacker, &admin_2),
            Err(QuickLendXError::NotAdmin)
        );

        initiate_admin_transfer(&env, &contract_id, &admin_1, &admin_2).unwrap();
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2));
        assert!(is_transfer_locked(&env, &contract_id));
    }

    #[test]
    fn initiate_transfer_before_initialization_and_self_target_are_rejected() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);

        assert_eq!(
            initiate_admin_transfer(&env, &contract_id, &admin, &admin),
            Err(QuickLendXError::OperationNotAllowed)
        );

        initialize_admin(&env, &contract_id, &admin).unwrap();
        assert_eq!(
            initiate_admin_transfer(&env, &contract_id, &admin, &admin),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn two_step_accept_requires_pending_admin() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let attacker = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();

        assert_eq!(
            accept_admin_transfer(&env, &contract_id, &attacker),
            Err(QuickLendXError::Unauthorized)
        );

        accept_admin_transfer(&env, &contract_id, &admin_2).unwrap();
        assert_eq!(get_admin(&env, &contract_id), Some(admin_2));
        assert_eq!(get_pending_admin(&env, &contract_id), None);
        assert!(!is_transfer_locked(&env, &contract_id));
    }

    #[test]
    fn accept_transfer_requires_initialized_state() {
        let (env, contract_id) = setup();
        let pending_admin = Address::generate(&env);

        assert_eq!(
            accept_admin_transfer(&env, &contract_id, &pending_admin),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn two_step_cancel_clears_pending_and_lock() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();
        cancel_admin_transfer(&env, &contract_id, &admin_1).unwrap();

        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
        assert_eq!(get_pending_admin(&env, &contract_id), None);
        assert!(!is_transfer_locked(&env, &contract_id));
    }

    #[test]
    fn transfer_lock_blocks_reentrant_transfer_attempts() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let admin_3 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();

        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &admin_3),
            Err(QuickLendXError::OperationNotAllowed)
        );
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2));
    }

    #[test]
    fn lock_and_pending_block_one_step_transfer_even_without_two_step_mode() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let pending_admin = Address::generate(&env);
        let replacement = Address::generate(&env);

        initiate_admin_transfer(&env, &contract_id, &admin_1, &pending_admin).unwrap();
        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &replacement),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn disabling_two_step_clears_stuck_pending_state() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();

        set_two_step_enabled(&env, &contract_id, &admin_1, false).unwrap();

        assert_eq!(get_pending_admin(&env, &contract_id), None);
        assert!(!is_transfer_locked(&env, &contract_id));
        assert!(!is_two_step_enabled(&env, &contract_id));
        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
    }

    #[test]
    fn emits_expected_events_for_admin_lifecycle() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_init"));

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_2st"));

        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_req"));

        accept_admin_transfer(&env, &contract_id, &admin_2).unwrap();
        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_trf"));
    }

    #[test]
    fn emits_cancel_event_when_pending_transfer_is_aborted() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = Address::generate(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();

        cancel_admin_transfer(&env, &contract_id, &admin_1).unwrap();

        assert_eq!(latest_topic_symbol(&env), symbol_short!("adm_cnl"));
        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
    }

    #[test]
    fn legacy_and_helper_paths_enforce_auth_and_state() {
        let (env, contract_id) = setup();
        let admin_1 = Address::generate(&env);
        let admin_2 = Address::generate(&env);
        let outsider = Address::generate(&env);

        // require_admin path before initialization
        assert_eq!(
            require_admin_auth(&env, &contract_id, &admin_1),
            Err(QuickLendXError::OperationNotAllowed)
        );
        assert_eq!(
            require_current_admin(&env, &contract_id),
            Err(QuickLendXError::OperationNotAllowed)
        );

        // set_admin_legacy initializes if not initialized.
        set_admin_legacy(&env, &contract_id, &admin_1).unwrap();
        assert_eq!(get_admin(&env, &contract_id), Some(admin_1.clone()));

        // set_admin alias delegates to transfer and updates admin.
        set_admin_alias(&env, &contract_id, &admin_1, &admin_2).unwrap();
        assert_eq!(get_admin(&env, &contract_id), Some(admin_2.clone()));

        // require_admin_auth for non-admin fails.
        assert_eq!(
            require_admin_auth(&env, &contract_id, &outsider),
            Err(QuickLendXError::NotAdmin)
        );

        // require_current_admin returns the current admin.
        assert_eq!(
            require_current_admin(&env, &contract_id),
            Ok(admin_2.clone())
        );

        // with_admin_auth and with_current_admin execute closures and propagate errors.
        assert_eq!(with_admin_auth_ok(&env, &contract_id, &admin_2), Ok(42));
        assert_eq!(
            with_admin_auth_err(&env, &contract_id, &admin_2),
            Err(QuickLendXError::OperationNotAllowed)
        );
        assert_eq!(
            with_current_admin_ok(&env, &contract_id),
            Ok(admin_2.clone())
        );
        assert_eq!(
            with_current_admin_err(&env, &contract_id),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn legacy_transfer_requires_existing_admin_when_initialized_flag_is_set_without_admin() {
        let (env, contract_id) = setup();
        let candidate = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);
        });

        assert_eq!(
            set_admin_legacy(&env, &contract_id, &candidate),
            Err(QuickLendXError::NotAdmin)
        );
    }
}
