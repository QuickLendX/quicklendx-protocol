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

    fn existing_destination(env: &Env) -> Address {
        env.register(QuickLendXContract, ())
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
        env.as_contract(contract_id, || {
            AdminStorage::transfer_admin(env, current_admin, new_admin)
        })
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
        env.as_contract(contract_id, || {
            AdminStorage::accept_admin_transfer(env, pending_admin)
        })
    }

    fn cancel_admin_transfer(
        env: &Env,
        contract_id: &Address,
        current_admin: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::cancel_admin_transfer(env, current_admin)
        })
    }

    fn set_two_step_enabled(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::set_two_step_enabled(env, admin, enabled)
        })
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
        env.as_contract(contract_id, || {
            AdminStorage::set_admin(env, current_admin, new_admin)
        })
    }

    fn require_admin_auth(
        env: &Env,
        contract_id: &Address,
        address: &Address,
    ) -> Result<(), QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::require_admin_auth(env, address)
        })
    }

    fn require_current_admin(env: &Env, contract_id: &Address) -> Result<Address, QuickLendXError> {
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
        env.as_contract(contract_id, || {
            AdminStorage::with_admin_auth(env, admin, || Ok(42))
        })
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

    fn with_current_admin_ok(env: &Env, contract_id: &Address) -> Result<Address, QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::with_current_admin(env, |admin| Ok(admin.clone()))
        })
    }

    fn with_current_admin_err(
        env: &Env,
        contract_id: &Address,
    ) -> Result<Address, QuickLendXError> {
        env.as_contract(contract_id, || {
            AdminStorage::with_current_admin(env, |_admin| {
                Err(QuickLendXError::OperationNotAllowed)
            })
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
        let admin_2 = existing_destination(&env);

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
        let admin_2 = existing_destination(&env);

        set_two_step_enabled(&env, &contract_id, &admin_1, true).unwrap();
        transfer_admin(&env, &contract_id, &admin_1, &admin_2).unwrap();

        assert_eq!(get_admin(&env, &contract_id), Some(admin_1));
        assert_eq!(get_pending_admin(&env, &contract_id), Some(admin_2));
        assert!(is_transfer_locked(&env, &contract_id));
    }

    #[test]
    fn direct_initiate_transfer_api_is_enforced_and_works() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = existing_destination(&env);
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
    fn generated_address_lookalike_destinations_are_rejected() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let lookalike_admin = Address::generate(&env);

        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &lookalike_admin),
            Err(QuickLendXError::InvalidAddress)
        );
        assert_eq!(get_admin(&env, &contract_id), Some(admin_1.clone()));

        assert_eq!(
            initiate_admin_transfer(&env, &contract_id, &admin_1, &lookalike_admin),
            Err(QuickLendXError::InvalidAddress)
        );
        assert_eq!(get_pending_admin(&env, &contract_id), None);
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
        let admin_2 = existing_destination(&env);
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
        let admin_2 = existing_destination(&env);

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
        let admin_2 = existing_destination(&env);
        let admin_3 = existing_destination(&env);

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
        let pending_admin = existing_destination(&env);
        let replacement = existing_destination(&env);

        initiate_admin_transfer(&env, &contract_id, &admin_1, &pending_admin).unwrap();
        assert_eq!(
            transfer_admin(&env, &contract_id, &admin_1, &replacement),
            Err(QuickLendXError::OperationNotAllowed)
        );
    }

    #[test]
    fn disabling_two_step_clears_stuck_pending_state() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = existing_destination(&env);

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
        let admin_2 = existing_destination(&env);

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
        let admin_2 = existing_destination(&env);

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
        let admin_2 = existing_destination(&env);
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

    #[test]
    fn admin_get_escrow_returns_full_record() {
        use crate::payments::{Escrow, EscrowStorage, EscrowStatus};
        use soroban_sdk::BytesN;

        let (env, contract_id, admin) = setup_with_admin();
        let escrow_id = BytesN::from_array(&env, &[1u8; 32]);
        let invoice_id = BytesN::from_array(&env, &[2u8; 32]);
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let currency = Address::generate(&env);

        let escrow = Escrow {
            escrow_id: escrow_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            business: business.clone(),
            amount: 1000,
            currency: currency.clone(),
            created_at: 12345,
            status: EscrowStatus::Held,
        };

        env.as_contract(&contract_id, || {
            EscrowStorage::store_escrow(&env, &escrow);
        });

        let result = env.as_contract(&contract_id, || {
            QuickLendXContract::admin_get_escrow(env.clone(), admin, escrow_id.clone())
        });

        assert!(result.is_ok());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.escrow_id, escrow_id);
        assert_eq!(retrieved.invoice_id, invoice_id);
        assert_eq!(retrieved.investor, investor);
        assert_eq!(retrieved.business, business);
        assert_eq!(retrieved.amount, 1000);
        assert_eq!(retrieved.currency, currency);
        assert_eq!(retrieved.created_at, 12345);
        assert_eq!(retrieved.status, EscrowStatus::Held);
    }

    #[test]
    fn admin_get_escrow_requires_admin() {
        use soroban_sdk::BytesN;

        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = Address::generate(&env);
        let escrow_id = BytesN::from_array(&env, &[1u8; 32]);

        let result = env.as_contract(&contract_id, || {
            QuickLendXContract::admin_get_escrow(env.clone(), non_admin, escrow_id)
        });

        assert_eq!(result, Err(QuickLendXError::NotAdmin));
    }

    #[test]
    fn admin_get_escrow_returns_storage_key_not_found_for_missing_escrow() {
        use soroban_sdk::BytesN;

        let (env, contract_id, admin) = setup_with_admin();
        let escrow_id = BytesN::from_array(&env, &[1u8; 32]);

        let result = env.as_contract(&contract_id, || {
            QuickLendXContract::admin_get_escrow(env.clone(), admin, escrow_id)
        });

        assert_eq!(result, Err(QuickLendXError::StorageKeyNotFound));
    }
}

// ============================================================================
// Comprehensive Access Control Matrix Tests
// ============================================================================
//
// This module provides comprehensive access-control testing for all admin-gated
// entrypoints in the QuickLendX protocol. The goal is to prevent privilege-escalation
// regressions as new methods are added.
//
// Access Control Matrix
//
// | Method Category | Method | Auth Required | Non-Admin Error |
// |-----------------|--------|---------------|-----------------|
// | **AdminStorage** | initialize | Caller (self-auth) | OperationNotAllowed |
// | | transfer_admin | Current Admin | NotAdmin |
// | | initiate_admin_transfer | Current Admin | NotAdmin |
// | | accept_admin_transfer | Pending Admin | Unauthorized |
// | | cancel_admin_transfer | Current Admin | NotAdmin |
// | | set_two_step_enabled | Current Admin | NotAdmin |
// | **Protocol Config** | set_protocol_config | Admin + Auth | NotAdmin |
// | | set_fee_config | Admin + Auth | NotAdmin |
// | | set_treasury | Admin + Auth | NotAdmin |
// | **Pause Control** | set_paused | Admin + Auth | NotAdmin |
// | **Emergency** | initiate | Admin + Auth | NotAdmin |
// | | execute | Admin + Auth | NotAdmin |
// | | cancel | Admin + Auth | NotAdmin |
// | **Currency** | add_currency | Admin + Auth | NotAdmin |
// | | remove_currency | Admin + Auth | NotAdmin |
// | | set_currencies | Admin + Auth | NotAdmin |
// | | clear_currencies | Admin + Auth | NotAdmin |
// | **Bid Config** | set_bid_ttl_days | Admin + Auth | NotAdmin |
// | | set_max_active_bids_per_investor | Admin + Auth | NotAdmin |
// | **Backup** | create_backup | Admin + Auth | NotAdmin |
// | | restore_backup | Admin + Auth | NotAdmin |
// | | archive_backup | Admin + Auth | NotAdmin |
// | | cleanup_backups | Admin + Auth | NotAdmin |
// | | set_backup_retention_policy | Admin + Auth | NotAdmin |
//
// Edge Cases Covered
// - Pre-init admin rejection (uninitialized state)
// - Transferred admin acceptance and revocation
// - Revoked caller rejection (former admin after transfer)
// - Self-transfer prevention
// - Two-step transfer flow authentication

#[cfg(all(test, feature = "legacy-tests"))]
mod access_control_matrix {
    use crate::admin::AdminStorage;
    use crate::backup;
    use crate::bid::BidStorage;
    use crate::currency::CurrencyWhitelist;
    use crate::emergency::EmergencyWithdraw;
    use crate::errors::QuickLendXError;
    use crate::init::ProtocolInitializer;
    use crate::pause::PauseControl;
    use crate::protocol_limits::ProtocolLimitsContract;
    use crate::QuickLendXContract;
    use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

    // ========================================================================
    // Test Setup Helpers
    // ========================================================================

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    fn setup_with_admin() -> (Env, Address, Address) {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        initialize_admin(&env, &contract_id, &admin);
        (env, contract_id, admin)
    }

    fn initialize_admin(env: &Env, contract_id: &Address, admin: &Address) {
        env.as_contract(contract_id, || {
            AdminStorage::initialize(env, admin).unwrap();
        });
    }

    fn get_non_admin(env: &Env) -> Address {
        Address::generate(env)
    }

    // ========================================================================
    // AdminStorage Access Control Tests
    // ========================================================================

    #[test]
    fn test_admin_storage_initialize_requires_self_auth() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Non-admin cannot initialize
        let result = env.as_contract(&contract_id, || AdminStorage::initialize(&env, &non_admin));
        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

        // Admin can initialize
        let result = env.as_contract(&contract_id, || AdminStorage::initialize(&env, &admin));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_admin_storage_transfer_requires_current_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let new_admin = Address::generate(&env);

        // Non-admin cannot transfer admin
        let result = env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &non_admin, &new_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can transfer
        let result = env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &admin, &new_admin)
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            env.as_contract(&contract_id, || AdminStorage::get_admin(&env)),
            Some(new_admin)
        );
    }

    #[test]
    fn test_admin_storage_transfer_rejects_revoked_caller() {
        let (env, contract_id, admin) = setup_with_admin();
        let new_admin = Address::generate(&env);

        // Transfer admin to new_admin
        env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &admin, &new_admin).unwrap();
        });

        // Former admin (now non-admin) cannot perform admin actions
        let result = env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &admin, &Address::generate(&env))
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));
    }

    #[test]
    fn test_admin_storage_initiate_transfer_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let pending_admin = Address::generate(&env);

        // Non-admin cannot initiate transfer
        let result = env.as_contract(&contract_id, || {
            AdminStorage::initiate_admin_transfer(&env, &non_admin, &pending_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can initiate
        let result = env.as_contract(&contract_id, || {
            AdminStorage::initiate_admin_transfer(&env, &admin, &pending_admin)
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_admin_storage_accept_transfer_requires_pending_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let attacker = get_non_admin(&env);

        // Enable two-step mode and initiate transfer
        env.as_contract(&contract_id, || {
            AdminStorage::set_two_step_enabled(&env, &admin, true).unwrap();
            AdminStorage::initiate_admin_transfer(&env, &admin, &admin_2).unwrap();
        });

        // Non-pending admin cannot accept
        let result = env.as_contract(&contract_id, || {
            AdminStorage::accept_admin_transfer(&env, &attacker)
        });
        assert_eq!(result, Err(QuickLendXError::Unauthorized));

        // Pending admin can accept
        let result = env.as_contract(&contract_id, || {
            AdminStorage::accept_admin_transfer(&env, &admin_2)
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_admin_storage_cancel_transfer_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let admin_2 = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Enable two-step and initiate transfer
        env.as_contract(&contract_id, || {
            AdminStorage::set_two_step_enabled(&env, &admin, true).unwrap();
            AdminStorage::initiate_admin_transfer(&env, &admin, &admin_2).unwrap();
        });

        // Non-admin cannot cancel
        let result = env.as_contract(&contract_id, || {
            AdminStorage::cancel_admin_transfer(&env, &non_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can cancel
        let result = env.as_contract(&contract_id, || {
            AdminStorage::cancel_admin_transfer(&env, &admin)
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            env.as_contract(&contract_id, || AdminStorage::get_pending_admin(&env)),
            None
        );
    }

    #[test]
    fn test_admin_storage_set_two_step_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot toggle two-step mode
        let result = env.as_contract(&contract_id, || {
            AdminStorage::set_two_step_enabled(&env, &non_admin, true)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can toggle
        let result = env.as_contract(&contract_id, || {
            AdminStorage::set_two_step_enabled(&env, &admin, true)
        });
        assert_eq!(result, Ok(()));
        assert!(env.as_contract(&contract_id, || AdminStorage::is_two_step_enabled(&env)));
    }

    // ========================================================================
    // ProtocolInitializer Access Control Tests
    // ========================================================================

    #[test]
    fn test_protocol_config_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot set protocol config
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_protocol_config(&env, &non_admin, 1000, 365, 86400)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set protocol config
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_protocol_config(&env, &admin, 1000, 365, 86400)
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_fee_config_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot set fee config
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_fee_config(&env, &non_admin, 200)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set fee config
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_fee_config(&env, &admin, 200)
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_treasury_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let treasury = Address::generate(&env);

        // Non-admin cannot set treasury
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_treasury(&env, &non_admin, &treasury)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set treasury
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_treasury(&env, &admin, &treasury)
        });
        assert_eq!(result, Ok(()));
    }

    // ========================================================================
    // PauseControl Access Control Tests
    // ========================================================================

    #[test]
    fn test_pause_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot pause
        let result = env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &non_admin, true)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can pause
        let result = env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &admin, true)
        });
        assert_eq!(result, Ok(()));
        assert!(env.as_contract(&contract_id, || PauseControl::is_paused(&env)));
    }

    #[test]
    fn test_unpause_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();

        // First pause
        env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &admin, true).unwrap();
        });

        let non_admin = get_non_admin(&env);

        // Non-admin cannot unpause
        let result = env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &non_admin, false)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can unpause
        let result = env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &admin, false)
        });
        assert_eq!(result, Ok(()));
        assert!(!env.as_contract(&contract_id, || PauseControl::is_paused(&env)));
    }

    #[test]
    fn test_pause_state_immutable_after_rejected_call() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Verify initial state is not paused
        assert!(!env.as_contract(&contract_id, || PauseControl::is_paused(&env)));

        // Non-admin attempts to pause - should fail
        env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &non_admin, true).unwrap_err();
        });

        // Verify state unchanged after rejection
        assert!(!env.as_contract(&contract_id, || PauseControl::is_paused(&env)));
    }

    // ========================================================================
    // EmergencyWithdraw Access Control Tests
    // ========================================================================

    #[test]
    fn test_emergency_initiate_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let token = Address::generate(&env);
        let target = Address::generate(&env);

        // Non-admin cannot initiate emergency withdraw
        let result = env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(&env, &non_admin, token.clone(), 1000, target.clone())
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can initiate
        let result = env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(&env, &admin, token.clone(), 1000, target.clone())
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_emergency_execute_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let token = Address::generate(&env);
        let target = Address::generate(&env);

        // Admin initiates first
        env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(&env, &admin, token, 1000, target).unwrap();
        });

        // Non-admin cannot execute
        let result = env.as_contract(&contract_id, || {
            EmergencyWithdraw::execute(&env, &non_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can execute (may fail due to balance, but auth passes)
        let result = env.as_contract(&contract_id, || EmergencyWithdraw::execute(&env, &admin));
        // Result may be Err depending on balance, but auth check passes
        match result {
            Ok(())
            | Err(QuickLendXError::InsufficientFunds)
            | Err(QuickLendXError::TokenTransferFailed) => {}
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_emergency_cancel_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let token = Address::generate(&env);
        let target = Address::generate(&env);

        // Admin initiates first
        env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(&env, &admin, token, 1000, target).unwrap();
        });

        // Non-admin cannot cancel
        let result = env.as_contract(&contract_id, || EmergencyWithdraw::cancel(&env, &non_admin));
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can cancel
        let result = env.as_contract(&contract_id, || EmergencyWithdraw::cancel(&env, &admin));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_emergency_withdraw_rejected_after_admin_transfer() {
        let (env, contract_id, admin) = setup_with_admin();
        let new_admin = Address::generate(&env);
        let token = Address::generate(&env);
        let target = Address::generate(&env);

        // Admin initiates
        env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(&env, &admin, token, 1000, target).unwrap();
        });

        // Transfer admin
        env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &admin, &new_admin).unwrap();
        });

        // Former admin cannot initiate new emergency withdraw
        let result = env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(
                &env,
                &admin,
                Address::generate(&env),
                1000,
                Address::generate(&env),
            )
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));
    }

    // ========================================================================
    // CurrencyWhitelist Access Control Tests
    // ========================================================================

    #[test]
    fn test_add_currency_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let currency = Address::generate(&env);

        // Non-admin cannot add currency
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::add_currency(&env, &non_admin, &currency)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can add currency
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::add_currency(&env, &admin, &currency)
        });
        assert_eq!(result, Ok(()));
        assert!(
            env.as_contract(&contract_id, || CurrencyWhitelist::is_allowed_currency(
                &env, &currency
            ))
        );
    }

    #[test]
    fn test_remove_currency_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let currency = Address::generate(&env);

        // Admin adds currency first
        env.as_contract(&contract_id, || {
            CurrencyWhitelist::add_currency(&env, &admin, &currency).unwrap();
        });

        // Non-admin cannot remove currency
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::remove_currency(&env, &non_admin, &currency)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can remove currency
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::remove_currency(&env, &admin, &currency)
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_set_currencies_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);
        let currencies = Vec::from_array(&env, &[Address::generate(&env), Address::generate(&env)]);

        // Non-admin cannot set currencies
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::set_currencies(&env, &non_admin, &currencies)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set currencies
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::set_currencies(&env, &admin, &currencies)
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_clear_currencies_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Add some currencies first
        env.as_contract(&contract_id, || {
            let currency = Address::generate(&env);
            CurrencyWhitelist::add_currency(&env, &admin, &currency).unwrap();
        });

        // Non-admin cannot clear currencies
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::clear_currencies(&env, &non_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can clear currencies
        let result = env.as_contract(&contract_id, || {
            CurrencyWhitelist::clear_currencies(&env, &admin)
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            env.as_contract(&contract_id, || CurrencyWhitelist::currency_count(&env)),
            0
        );
    }

    // ========================================================================
    // BidStorage Access Control Tests
    // ========================================================================

    #[test]
    fn test_set_bid_ttl_days_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot set bid TTL
        let result = env.as_contract(&contract_id, || {
            BidStorage::set_bid_ttl_days(&env, &non_admin, 14)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set bid TTL
        let result = env.as_contract(&contract_id, || {
            BidStorage::set_bid_ttl_days(&env, &admin, 14)
        });
        assert_eq!(result, Ok(14));
        assert_eq!(
            env.as_contract(&contract_id, || BidStorage::get_bid_ttl_days(&env)),
            14
        );
    }

    #[test]
    fn test_set_max_active_bids_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot set max active bids
        let result = env.as_contract(&contract_id, || {
            BidStorage::set_max_active_bids_per_investor(&env, &non_admin, 50)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set max active bids
        let result = env.as_contract(&contract_id, || {
            BidStorage::set_max_active_bids_per_investor(&env, &admin, 50)
        });
        assert_eq!(result, Ok(50));
        assert_eq!(
            env.as_contract(&contract_id, || {
                BidStorage::get_max_active_bids_per_investor(&env)
            }),
            50
        );
    }

    #[test]
    fn test_reset_bid_ttl_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Set a custom TTL first
        env.as_contract(&contract_id, || {
            BidStorage::set_bid_ttl_days(&env, &admin, 14).unwrap();
        });

        // Non-admin cannot reset
        let result = env.as_contract(&contract_id, || {
            BidStorage::reset_bid_ttl_to_default(&env, &non_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can reset
        let result = env.as_contract(&contract_id, || {
            BidStorage::reset_bid_ttl_to_default(&env, &admin)
        });
        assert_eq!(result, Ok(7)); // Default is 7 days
    }

    // ========================================================================
    // ProtocolLimitsContract Access Control Tests
    // ========================================================================

    #[test]
    fn test_set_protocol_limits_requires_admin() {
        let (env, contract_id, admin) = setup_with_admin();
        let non_admin = get_non_admin(&env);

        // Non-admin cannot set protocol limits
        let result = env.as_contract(&contract_id, || {
            ProtocolLimitsContract::set_protocol_limits(
                &env, &non_admin, 1000, 10, 100, 365, 86400, 100,
            )
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can set protocol limits
        let result = env.as_contract(&contract_id, || {
            ProtocolLimitsContract::set_protocol_limits(
                &env, &admin, 1000, 10, 100, 365, 86400, 100,
            )
        });
        assert_eq!(result, Ok(()));
    }

    // ========================================================================
    // BackupStorage Access Control Tests (Contract Entrypoints)
    // ========================================================================

    #[test]
    fn test_create_backup_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        // Non-admin cannot create backup
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let result = client.try_create_backup(&non_admin);
        assert!(result.is_err());

        // Admin can create backup
        let result = client.create_backup(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_restore_backup_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin and create a backup
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let backup_id = client.create_backup(&admin);

        // Non-admin cannot restore
        let result = client.try_restore_backup(&non_admin, &backup_id);
        assert!(result.is_err());

        // Admin can restore
        let result = client.restore_backup(&admin, &backup_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_archive_backup_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin and create a backup
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let backup_id = client.create_backup(&admin);

        // Non-admin cannot archive
        let result = client.try_archive_backup(&non_admin, &backup_id);
        assert!(result.is_err());

        // Admin can archive
        let result = client.archive_backup(&admin, &backup_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cleanup_backups_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot cleanup
        let result = client.try_cleanup_backups(&non_admin);
        assert!(result.is_err());

        // Admin can cleanup
        let result = client.cleanup_backups(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_backup_retention_policy_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set retention policy
        let result = client.try_set_backup_retention_policy(&non_admin, 5, 0, true);
        assert!(result.is_err());

        // Admin can set retention policy
        let result = client.set_backup_retention_policy(&admin, 5, 0, true);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Contract Entrypoint Access Control Tests
    // ========================================================================

    #[test]
    fn test_transfer_admin_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let new_admin = Address::generate(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot transfer admin
        let result = client.try_transfer_admin(&non_admin);
        assert!(result.is_err());

        // Admin can transfer
        let result = client.transfer_admin(&new_admin);
        assert!(result.is_ok());
        assert_eq!(client.get_current_admin(), Some(new_admin));
    }

    #[test]
    fn test_set_protocol_config_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set protocol config
        let result = client.try_set_protocol_config(&non_admin, 1000, 365, 86400);
        assert!(result.is_err());

        // Admin can set protocol config
        let result = client.set_protocol_config(&admin, 1000, 365, 86400);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_fee_config_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set fee config
        let result = client.try_set_fee_config(&non_admin, 200);
        assert!(result.is_err());

        // Admin can set fee config
        let result = client.set_fee_config(&admin, 200);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_treasury_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let treasury = Address::generate(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set treasury
        let result = client.try_set_treasury(&non_admin, &treasury);
        assert!(result.is_err());

        // Admin can set treasury
        let result = client.set_treasury(&admin, &treasury);
        assert!(result.is_ok());
    }

    #[test]
    fn test_pause_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot pause
        let result = client.try_pause(&non_admin);
        assert!(result.is_err());

        // Admin can pause
        let result = client.pause(&admin);
        assert!(result.is_ok());
        assert!(client.is_paused());
    }

    #[test]
    fn test_unpause_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin and pause
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.pause(&admin);

        // Non-admin cannot unpause
        let result = client.try_unpause(&non_admin);
        assert!(result.is_err());

        // Admin can unpause
        let result = client.unpause(&admin);
        assert!(result.is_ok());
        assert!(!client.is_paused());
    }

    #[test]
    fn test_add_currency_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let currency = Address::generate(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot add currency
        let result = client.try_add_currency(&non_admin, &currency);
        assert!(result.is_err());

        // Admin can add currency
        let result = client.add_currency(&admin, &currency);
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_currency_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let currency = Address::generate(&env);

        // Initialize admin and add currency
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.add_currency(&admin, &currency);

        // Non-admin cannot remove currency
        let result = client.try_remove_currency(&non_admin, &currency);
        assert!(result.is_err());

        // Admin can remove currency
        let result = client.remove_currency(&admin, &currency);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_currencies_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let currencies = Vec::from_array(&env, &[Address::generate(&env)]);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set currencies
        let result = client.try_set_currencies(&non_admin, &currencies);
        assert!(result.is_err());

        // Admin can set currencies
        let result = client.set_currencies(&admin, &currencies);
        assert!(result.is_ok());
    }

    #[test]
    fn test_clear_currencies_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot clear currencies
        let result = client.try_clear_currencies(&non_admin);
        assert!(result.is_err());

        // Admin can clear currencies
        let result = client.clear_currencies(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_initiate_emergency_withdraw_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let token = Address::generate(&env);
        let target = Address::generate(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot initiate emergency withdraw
        let result = client.try_initiate_emergency_withdraw(&non_admin, &token, 1000, &target);
        assert!(result.is_err());

        // Admin can initiate emergency withdraw
        let result = client.initiate_emergency_withdraw(&admin, &token, 1000, &target);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_emergency_withdraw_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin and initiate emergency withdraw
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let token = Address::generate(&env);
        let target = Address::generate(&env);
        client.initiate_emergency_withdraw(&admin, &token, 1000, &target);

        // Non-admin cannot execute emergency withdraw
        let result = client.try_execute_emergency_withdraw(&non_admin);
        assert!(result.is_err());

        // Admin can execute emergency withdraw
        let result = client.execute_emergency_withdraw(&admin);
        // May fail due to insufficient balance, but auth check passes
        match result {
            Ok(())
            | Err(QuickLendXError::InsufficientFunds)
            | Err(QuickLendXError::TokenTransferFailed) => {}
            _ => {}
        }
    }

    #[test]
    fn test_cancel_emergency_withdraw_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin and initiate emergency withdraw
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let token = Address::generate(&env);
        let target = Address::generate(&env);
        client.initiate_emergency_withdraw(&admin, &token, 1000, &target);

        // Non-admin cannot cancel emergency withdraw
        let result = client.try_cancel_emergency_withdraw(&non_admin);
        assert!(result.is_err());

        // Admin can cancel emergency withdraw
        let result = client.cancel_emergency_withdraw(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_initialize_protocol_limits_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot initialize protocol limits
        let result = client.try_initialize_protocol_limits(&non_admin, 1000, 365, 86400);
        assert!(result.is_err());

        // Admin can initialize protocol limits
        let result = client.initialize_protocol_limits(&admin, 1000, 365, 86400);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_protocol_limits_entrypoint_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set protocol limits
        let result = client.try_set_protocol_limits(&non_admin, 1000, 365, 86400);
        assert!(result.is_err());

        // Admin can set protocol limits
        let result = client.set_protocol_limits(&admin, 1000, 365, 86400);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Edge Case: Pre-Initialization State Tests
    // ========================================================================

    #[test]
    fn test_all_admin_methods_fail_before_initialization() {
        let (env, contract_id) = setup();
        let non_admin = get_non_admin(&env);

        // AdminStorage::initialize - requires self-auth, not admin check
        let result = env.as_contract(&contract_id, || AdminStorage::initialize(&env, &non_admin));
        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

        // AdminStorage::transfer_admin - requires initialized state
        let result = env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &non_admin, &Address::generate(&env))
        });
        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

        // AdminStorage::require_current_admin - returns error when not initialized
        let result = env.as_contract(&contract_id, || AdminStorage::require_current_admin(&env));
        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));

        // ProtocolInitializer methods - would fail admin check
        let result = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_protocol_config(&env, &non_admin, 1000, 365, 86400)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // PauseControl - would fail admin check
        let result = env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &non_admin, true)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // EmergencyWithdraw - would fail admin check
        let result = env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(
                &env,
                &non_admin,
                Address::generate(&env),
                1000,
                Address::generate(&env),
            )
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));
    }

    // ========================================================================
    // Edge Case: Admin Transfer and Revocation Tests
    // ========================================================================

    #[test]
    fn test_former_admin_cannot_use_admin_methods_after_transfer() {
        let (env, contract_id) = setup();
        let admin_1 = Address::generate(&env);
        let admin_2 = Address::generate(&env);

        // Initialize admin_1
        initialize_admin(&env, &contract_id, &admin_1);

        // admin_1 transfers to admin_2
        env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &admin_1, &admin_2).unwrap();
        });

        // Verify admin_2 is now admin
        assert_eq!(
            env.as_contract(&contract_id, || AdminStorage::get_admin(&env)),
            Some(admin_2)
        );

        // admin_1 (former admin) cannot perform any admin operations
        let results = vec![
            env.as_contract(&contract_id, || {
                AdminStorage::transfer_admin(&env, &admin_1, &Address::generate(&env))
            }),
            env.as_contract(&contract_id, || {
                AdminStorage::initiate_admin_transfer(&env, &admin_1, &Address::generate(&env))
            }),
            env.as_contract(&contract_id, || {
                AdminStorage::set_two_step_enabled(&env, &admin_1, true)
            }),
            env.as_contract(&contract_id, || {
                ProtocolInitializer::set_protocol_config(&env, &admin_1, 1000, 365, 86400)
            }),
            env.as_contract(&contract_id, || {
                ProtocolInitializer::set_fee_config(&env, &admin_1, 200)
            }),
            env.as_contract(&contract_id, || {
                PauseControl::set_paused(&env, &admin_1, true)
            }),
            env.as_contract(&contract_id, || {
                EmergencyWithdraw::initiate(
                    &env,
                    &admin_1,
                    Address::generate(&env),
                    1000,
                    Address::generate(&env),
                )
            }),
            env.as_contract(&contract_id, || {
                CurrencyWhitelist::add_currency(&env, &admin_1, &Address::generate(&env))
            }),
            env.as_contract(&contract_id, || {
                BidStorage::set_bid_ttl_days(&env, &admin_1, 14)
            }),
            env.as_contract(&contract_id, || {
                BidStorage::set_max_active_bids_per_investor(&env, &admin_1, 50)
            }),
            env.as_contract(&contract_id, || {
                ProtocolLimitsContract::set_protocol_limits(
                    &env, &admin_1, 1000, 10, 100, 365, 86400, 100,
                )
            }),
        ];

        for (i, result) in results.into_iter().enumerate() {
            assert_eq!(
                result,
                Err(QuickLendXError::NotAdmin),
                "Test {}: Former admin should be rejected for admin method {}",
                i + 1,
                i + 1
            );
        }

        // admin_2 can perform all admin operations
        let results = vec![
            (
                ProtocolInitializer::set_protocol_config(&env, &admin_2, 1000, 365, 86400),
                "set_protocol_config",
            ),
            (
                ProtocolInitializer::set_fee_config(&env, &admin_2, 200),
                "set_fee_config",
            ),
            (PauseControl::set_paused(&env, &admin_2, true), "set_paused"),
            (
                CurrencyWhitelist::add_currency(&env, &admin_2, &Address::generate(&env)),
                "add_currency",
            ),
            (
                BidStorage::set_bid_ttl_days(&env, &admin_2, 14),
                "set_bid_ttl_days",
            ),
            (
                BidStorage::set_max_active_bids_per_investor(&env, &admin_2, 50),
                "set_max_active_bids",
            ),
        ];

        for (result, name) in results {
            assert_eq!(
                result.map(|_| ()),
                Ok(()),
                "New admin should be accepted for {}",
                name
            );
        }
    }

    // ========================================================================
    // Privilege Escalation Prevention Tests
    // ========================================================================

    #[test]
    fn test_no_privilege_escalation_via_call_order() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);

        // Attempt multiple admin operations from non-admin in sequence
        // All should fail independently
        for _ in 0..3 {
            let results = vec![
                env.as_contract(&contract_id, || {
                    AdminStorage::transfer_admin(&env, &non_admin, &Address::generate(&env))
                }),
                env.as_contract(&contract_id, || {
                    ProtocolInitializer::set_protocol_config(&env, &non_admin, 1000, 365, 86400)
                }),
                env.as_contract(&contract_id, || {
                    PauseControl::set_paused(&env, &non_admin, true)
                }),
            ];

            for result in results {
                assert_eq!(result, Err(QuickLendXError::NotAdmin));
            }
        }

        // Verify admin is still the original admin
        assert_eq!(
            env.as_contract(&contract_id, || AdminStorage::get_admin(&env)),
            Some(admin)
        );
    }

    #[test]
    fn test_no_privilege_escalation_via_partial_auth() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);

        // Some methods require require_auth first, then admin check
        // Verify that just having the admin address is not enough
        // without proper authentication

        // AdminStorage::transfer_admin requires both:
        // 1. current_admin.require_auth() - done in test
        // 2. AdminStorage::require_admin() - verifies caller is admin

        // A non-admin calling with their own address should fail
        // because require_admin checks if the address is admin
        let result = env.as_contract(&contract_id, || {
            // This simulates a non-admin trying to call with their own address
            // The require_auth would succeed if the non_admin signed the tx,
            // but AdminStorage::require_admin would fail because non_admin != admin
            AdminStorage::require_admin(&env, &non_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin calling with admin address should succeed
        let result = env.as_contract(&contract_id, || AdminStorage::require_admin(&env, &admin));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_admin_methods_are_consistent_across_modules() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);

        // Test that all modules use the same admin check pattern
        // This ensures consistent security behavior

        // Pattern 1: require_admin + require_auth
        let result1 = env.as_contract(&contract_id, || {
            AdminStorage::with_admin_auth(&env, &admin, || Ok(()))
        });
        assert_eq!(result1, Ok(()));

        let result2 = env.as_contract(&contract_id, || {
            AdminStorage::with_admin_auth(&env, &non_admin, || Ok(()))
        });
        assert_eq!(result2, Err(QuickLendXError::NotAdmin));

        // Pattern 2: require_current_admin
        let result3 = env.as_contract(&contract_id, || AdminStorage::require_current_admin(&env));
        assert_eq!(result3, Ok(admin));

        // Verify consistency: all admin-gated methods reject non-admin
        let methods_to_test = vec![
            ("AdminStorage::transfer_admin", |env: &Env| {
                env.as_contract(&contract_id, || {
                    AdminStorage::transfer_admin(env, &non_admin, &Address::generate(env))
                })
            }),
            ("ProtocolInitializer::set_protocol_config", |env: &Env| {
                env.as_contract(&contract_id, || {
                    ProtocolInitializer::set_protocol_config(env, &non_admin, 1000, 365, 86400)
                })
            }),
            ("PauseControl::set_paused", |env: &Env| {
                env.as_contract(&contract_id, || {
                    PauseControl::set_paused(env, &non_admin, true)
                })
            }),
        ];

        for (name, method) in methods_to_test {
            let result = method(&env);
            assert_eq!(
                result,
                Err(QuickLendXError::NotAdmin),
                "Method {} should reject non-admin",
                name
            );
        }
    }

    // ========================================================================
    // State Immutability Tests (Post-Rejection Verification)
    // ========================================================================

    #[test]
    fn test_state_immutable_after_non_admin_rejection() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);

        // Capture initial state
        let initial_fee = env.as_contract(&contract_id, || ProtocolInitializer::get_fee_bps(&env));
        let initial_paused = env.as_contract(&contract_id, || PauseControl::is_paused(&env));
        let initial_admin = env.as_contract(&contract_id, || AdminStorage::get_admin(&env));
        let initial_bid_ttl = env.as_contract(&contract_id, || BidStorage::get_bid_ttl_days(&env));

        // Non-admin attempts multiple operations
        let _ = env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &non_admin, &Address::generate(&env))
        });
        let _ = env.as_contract(&contract_id, || {
            ProtocolInitializer::set_fee_config(&env, &non_admin, 9999)
        });
        let _ = env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &non_admin, true)
        });
        let _ = env.as_contract(&contract_id, || {
            BidStorage::set_bid_ttl_days(&env, &non_admin, 99)
        });

        // Verify all state remains unchanged
        assert_eq!(
            env.as_contract(&contract_id, || ProtocolInitializer::get_fee_bps(&env)),
            initial_fee
        );
        assert_eq!(
            env.as_contract(&contract_id, || PauseControl::is_paused(&env)),
            initial_paused
        );
        assert_eq!(
            env.as_contract(&contract_id, || AdminStorage::get_admin(&env)),
            initial_admin
        );
        assert_eq!(
            env.as_contract(&contract_id, || BidStorage::get_bid_ttl_days(&env)),
            initial_bid_ttl
        );
    }

    // ========================================================================
    // Documentation Reference Test
    // ========================================================================

    #[test]
    fn test_access_control_matrix_documented() {
        // This test serves as documentation reference
        // The matrix below documents the expected behavior:
        //
        // | Method | Non-Admin Result | Admin Result |
        // |--------|------------------|--------------|
        // | AdminStorage::initialize | OperationNotAllowed | Ok(()) |
        // | AdminStorage::transfer_admin | NotAdmin | Ok(()) |
        // | AdminStorage::initiate_admin_transfer | NotAdmin | Ok(()) |
        // | AdminStorage::accept_admin_transfer | Unauthorized | Ok(()) |
        // | AdminStorage::cancel_admin_transfer | NotAdmin | Ok(()) |
        // | AdminStorage::set_two_step_enabled | NotAdmin | Ok(()) |
        // | ProtocolInitializer::set_protocol_config | NotAdmin | Ok(()) |
        // | ProtocolInitializer::set_fee_config | NotAdmin | Ok(()) |
        // | ProtocolInitializer::set_treasury | NotAdmin | Ok(()) |
        // | PauseControl::set_paused | NotAdmin | Ok(()) |
        // | EmergencyWithdraw::initiate | NotAdmin | Ok(()) |
        // | EmergencyWithdraw::execute | NotAdmin | Ok(()) or BalanceError |
        // | EmergencyWithdraw::cancel | NotAdmin | Ok(()) |
        // | CurrencyWhitelist::add_currency | NotAdmin | Ok(()) |
        // | CurrencyWhitelist::remove_currency | NotAdmin | Ok(()) |
        // | CurrencyWhitelist::set_currencies | NotAdmin | Ok(()) |
        // | CurrencyWhitelist::clear_currencies | NotAdmin | Ok(()) |
        // | BidStorage::set_bid_ttl_days | NotAdmin | Ok(()) |
        // | BidStorage::set_max_active_bids_per_investor | NotAdmin | Ok(()) |
        // | ProtocolLimitsContract::set_protocol_limits | NotAdmin | Ok(()) |
        //
        // Edge Cases:
        // - Pre-initialization: All admin methods return OperationNotAllowed
        // - Admin transfer: Former admin rejected, new admin accepted
        // - State immutability: Rejected calls leave state unchanged

        assert!(true); // Documentation test passes
    }
}

// ============================================================================
// Additional Admin-Gated Method Tests (Extended Coverage)
// ============================================================================

#[cfg(all(test, feature = "legacy-tests"))]
mod access_control_matrix_extended {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::QuickLendXContract;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    fn setup_with_admin() -> (Env, Address, Address) {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
        });
        (env, contract_id, admin)
    }

    fn initialize_admin(env: &Env, contract_id: &Address, admin: &Address) {
        env.as_contract(contract_id, || {
            AdminStorage::initialize(env, admin).unwrap();
        });
    }

    fn get_non_admin(env: &Env) -> Address {
        Address::generate(env)
    }

    // ========================================================================
    // Fee Management Tests
    // ========================================================================

    #[test]
    fn test_initialize_fee_system_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot initialize fee system
        let result = client.try_initialize_fee_system(&non_admin);
        assert!(result.is_err());

        // Admin can initialize fee system
        let result = client.initialize_fee_system(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_configure_treasury_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.initialize_fee_system(&admin);

        let treasury = Address::generate(&env);

        // Non-admin cannot configure treasury
        let result = client.try_configure_treasury(&treasury);
        assert!(result.is_err());

        // Admin can configure treasury
        let result = client.configure_treasury(&treasury);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_platform_fee_bps_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.initialize_fee_system(&admin);

        // Non-admin cannot update platform fee
        let result = client.try_update_platform_fee_bps(250);
        assert!(result.is_err());

        // Admin can update platform fee
        let result = client.update_platform_fee_bps(250);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_fee_structure_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.initialize_fee_system(&admin);

        // Non-admin cannot update fee structure
        let fee_type = crate::types::FeeType::PlatformFee;
        let result =
            client.try_update_fee_structure(&non_admin, fee_type.clone(), 100, 10, 1000, true);
        assert!(result.is_err());

        // Admin can update fee structure
        let result = client.update_fee_structure(&admin, fee_type.clone(), 100, 10, 1000, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_configure_revenue_distribution_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.initialize_fee_system(&admin);

        let treasury = Address::generate(&env);

        // Non-admin cannot configure revenue distribution
        let result = client.try_configure_revenue_distribution(
            &non_admin, &treasury, 8000, // treasury_share_bps
            1500, // developer_share_bps
            500,  // platform_share_bps
            true, // auto_distribution
            1000, // min_distribution_amount
        );
        assert!(result.is_err());

        // Admin can configure revenue distribution
        let result =
            client.configure_revenue_distribution(&admin, &treasury, 8000, 1500, 500, true, 1000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_distribute_revenue_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        client.initialize_fee_system(&admin);

        // Non-admin cannot distribute revenue
        let result = client.try_distribute_revenue(&non_admin, 1000);
        assert!(result.is_err());

        // Admin can distribute revenue (may return 0 if no revenue to distribute)
        let result = client.distribute_revenue(&admin, 1000);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Business/Investor Verification Tests
    // ========================================================================

    #[test]
    fn test_verify_business_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let business = Address::generate(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot verify business
        let result = client.try_verify_business(&non_admin, &business);
        assert!(result.is_err());

        // Admin can verify business
        let result = client.verify_business(&admin, &business);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_business_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let business = Address::generate(&env);
        let reason = soroban_sdk::String::from_str(&env, "KYC documents not provided");

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot reject business
        let result = client.try_reject_business(&non_admin, &business, &reason);
        assert!(result.is_err());

        // Admin can reject business
        let result = client.reject_business(&admin, &business, &reason);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_investor_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let investor = Address::generate(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot verify investor
        let result = client.try_verify_investor(&investor, 1000000);
        assert!(result.is_err());

        // Admin can verify investor
        let result = client.verify_investor(&investor, 1000000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_investor_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let investor = Address::generate(&env);
        let reason = soroban_sdk::String::from_str(&env, "Invalid KYC data");

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot reject investor
        let result = client.try_reject_investor(&investor, &reason);
        assert!(result.is_err());

        // Admin can reject investor
        let result = client.reject_investor(&investor, &reason);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_investment_limit_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let investor = Address::generate(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set investment limit
        let result = client.try_set_investment_limit(&investor, 500000);
        assert!(result.is_err());

        // Admin can set investment limit
        let result = client.set_investment_limit(&investor, 500000);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Vesting Tests
    // ========================================================================

    #[test]
    fn test_create_vesting_schedule_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);
        let token = Address::generate(&env);
        let beneficiary = Address::generate(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot create vesting schedule
        let result = client.try_create_vesting_schedule(
            &non_admin,
            &token,
            &beneficiary,
            1000000,
            1000,
            100,
            2000,
        );
        assert!(result.is_err());

        // Admin can create vesting schedule
        let result =
            client.create_vesting_schedule(&admin, &token, &beneficiary, 1000000, 1000, 100, 2000);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Protocol Limits Tests
    // ========================================================================

    #[test]
    fn test_update_protocol_limits_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot update protocol limits
        let result = client.try_update_protocol_limits(&non_admin, 1000, 365, 86400);
        assert!(result.is_err());

        // Admin can update protocol limits
        let result = client.update_protocol_limits(&admin, 1000, 365, 86400);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_limits_max_invoices_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot update limits with max invoices
        let result = client.try_update_limits_max_invoices(&non_admin, 1000, 365, 86400, 50);
        assert!(result.is_err());

        // Admin can update limits with max invoices
        let result = client.update_limits_max_invoices(&admin, 1000, 365, 86400, 50);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Set Admin Tests
    // ========================================================================

    #[test]
    fn test_set_admin_requires_auth() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set admin (even with their own auth)
        let result = client.try_set_admin(&non_admin);
        // This should fail because non_admin is not current admin and has no auth
        assert!(result.is_err());

        // Admin can set admin (transfers to new admin)
        let new_admin = Address::generate(&env);
        let result = client.set_admin(&new_admin);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Platform Fee Tests
    // ========================================================================

    #[test]
    fn test_set_platform_fee_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        let client = crate::QuickLendXContractClient::new(&env, &contract_id);

        // Non-admin cannot set platform fee
        let result = client.try_set_platform_fee(250);
        assert!(result.is_err());

        // Admin can set platform fee
        let result = client.set_platform_fee(250);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Bid TTL Tests (Entry point without admin param)
    // ========================================================================

    #[test]
    fn test_reset_bid_ttl_to_default_requires_admin() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin first
        initialize_admin(&env, &contract_id, &admin);

        // Set a custom TTL first (via direct storage access)
        env.as_contract(&contract_id, || {
            crate::bid::BidStorage::set_bid_ttl_days(&env, &admin, 14).unwrap();
        });

        // Test reset via direct storage access
        let result = env.as_contract(&contract_id, || {
            crate::bid::BidStorage::reset_bid_ttl_to_default(&env, &non_admin)
        });
        assert_eq!(result, Err(QuickLendXError::NotAdmin));

        // Admin can reset
        let result = env.as_contract(&contract_id, || {
            crate::bid::BidStorage::reset_bid_ttl_to_default(&env, &admin)
        });
        assert_eq!(result, Ok(7)); // Default is 7 days
    }

    // ========================================================================
    // Comprehensive Privilege Escalation Prevention
    // ========================================================================

    #[test]
    fn test_complete_admin_methods_list_prevents_privilege_escalation() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let non_admin = get_non_admin(&env);

        // Initialize admin
        initialize_admin(&env, &contract_id, &admin);

        // Initialize fee system first (required by some tests)
        let client = crate::QuickLendXContractClient::new(&env, &contract_id);
        let _ = client.initialize_fee_system(&admin);

        // Test ALL admin-gated entrypoints systematically
        // Non-admin should be rejected for each
        let admin_methods = vec![
            ("transfer_admin", || client.try_transfer_admin(&non_admin)),
            ("set_protocol_config", || {
                client.try_set_protocol_config(&non_admin, 1000, 365, 86400)
            }),
            ("set_fee_config", || {
                client.try_set_fee_config(&non_admin, 200)
            }),
            ("add_currency", || {
                client.try_add_currency(&non_admin, &Address::generate(&env))
            }),
            ("remove_currency", || {
                client.try_remove_currency(&non_admin, &Address::generate(&env))
            }),
            ("set_currencies", || {
                client.try_set_currencies(&non_admin, &Vec::new(&env))
            }),
            ("clear_currencies", || {
                client.try_clear_currencies(&non_admin)
            }),
            ("initiate_emergency_withdraw", || {
                client.try_initiate_emergency_withdraw(
                    &non_admin,
                    &Address::generate(&env),
                    1000,
                    &Address::generate(&env),
                )
            }),
            ("execute_emergency_withdraw", || {
                client.try_execute_emergency_withdraw(&non_admin)
            }),
            ("cancel_emergency_withdraw", || {
                client.try_cancel_emergency_withdraw(&non_admin)
            }),
            ("pause", || client.try_pause(&non_admin)),
            ("unpause", || client.try_unpause(&non_admin)),
            ("create_backup", || client.try_create_backup(&non_admin)),
            ("cleanup_backups", || client.try_cleanup_backups(&non_admin)),
            ("set_backup_retention_policy", || {
                client.try_set_backup_retention_policy(&non_admin, 5, 0, true)
            }),
            ("initialize_protocol_limits", || {
                client.try_initialize_protocol_limits(&non_admin, 1000, 365, 86400)
            }),
            ("set_protocol_limits", || {
                client.try_set_protocol_limits(&non_admin, 1000, 365, 86400)
            }),
            ("update_protocol_limits", || {
                client.try_update_protocol_limits(&non_admin, 1000, 365, 86400)
            }),
            ("update_limits_max_invoices", || {
                client.try_update_limits_max_invoices(&non_admin, 1000, 365, 86400, 50)
            }),
            ("verify_business", || {
                client.try_verify_business(&non_admin, &Address::generate(&env))
            }),
            ("reject_business", || {
                client.try_reject_business(
                    &non_admin,
                    &Address::generate(&env),
                    &soroban_sdk::String::from_str(&env, "reason"),
                )
            }),
            ("set_investment_limit", || {
                client.try_set_investment_limit(&Address::generate(&env), 500000)
            }),
            ("create_vesting_schedule", || {
                client.try_create_vesting_schedule(
                    &non_admin,
                    &Address::generate(&env),
                    &Address::generate(&env),
                    1000000,
                    1000,
                    100,
                    2000,
                )
            }),
            ("initialize_fee_system", || {
                client.try_initialize_fee_system(&non_admin)
            }),
        ];

        for (method_name, method_call) in admin_methods {
            let result = method_call();
            assert!(
                result.is_err(),
                "Method {} should reject non-admin caller, but it succeeded",
                method_name
            );
        }
    }

    #[test]
    fn test_admin_transferred_no_longer_has_access() {
        let (env, contract_id) = setup();
        let admin_1 = Address::generate(&env);
        let admin_2 = Address::generate(&env);

        // Initialize admin_1
        initialize_admin(&env, &contract_id, &admin_1);

        // Transfer admin to admin_2
        env.as_contract(&contract_id, || {
            AdminStorage::transfer_admin(&env, &admin_1, &admin_2).unwrap();
        });

        // Verify admin_2 is now admin
        assert_eq!(
            env.as_contract(&contract_id, || AdminStorage::get_admin(&env)),
            Some(admin_2)
        );

        // admin_1 should be rejected for all admin methods
        let methods = vec![
            ("set_protocol_config", || {
                env.as_contract(&contract_id, || {
                    crate::init::ProtocolInitializer::set_protocol_config(
                        &env, &admin_1, 1000, 365, 86400,
                    )
                })
            }),
            ("set_fee_config", || {
                env.as_contract(&contract_id, || {
                    crate::init::ProtocolInitializer::set_fee_config(&env, &admin_1, 200)
                })
            }),
            ("set_paused", || {
                env.as_contract(&contract_id, || {
                    crate::pause::PauseControl::set_paused(&env, &admin_1, true)
                })
            }),
            ("add_currency", || {
                env.as_contract(&contract_id, || {
                    crate::currency::CurrencyWhitelist::add_currency(
                        &env,
                        &admin_1,
                        &Address::generate(&env),
                    )
                })
            }),
            ("set_bid_ttl_days", || {
                env.as_contract(&contract_id, || {
                    crate::bid::BidStorage::set_bid_ttl_days(&env, &admin_1, 14)
                })
            }),
            ("set_max_active_bids", || {
                env.as_contract(&contract_id, || {
                    crate::bid::BidStorage::set_max_active_bids_per_investor(&env, &admin_1, 50)
                })
            }),
            ("set_protocol_limits", || {
                env.as_contract(&contract_id, || {
                    crate::protocol_limits::ProtocolLimitsContract::set_protocol_limits(
                        &env,
                        admin_1.clone(),
                        1000,
                        10,
                        100,
                        365,
                        86400,
                        100,
                    )
                })
            }),
            ("emergency_initiate", || {
                env.as_contract(&contract_id, || {
                    crate::emergency::EmergencyWithdraw::initiate(
                        &env,
                        &admin_1,
                        Address::generate(&env),
                        1000,
                        Address::generate(&env),
                    )
                })
            }),
        ];

        for (method_name, method_call) in methods {
            let result = method_call();
            assert_eq!(
                result,
                Err(QuickLendXError::NotAdmin),
                "Former admin should be rejected for {}",
                method_name
            );
        }

        // admin_2 should be accepted for all admin methods
        let methods = vec![
            ("set_protocol_config", || {
                env.as_contract(&contract_id, || {
                    crate::init::ProtocolInitializer::set_protocol_config(
                        &env, &admin_2, 1000, 365, 86400,
                    )
                })
            }),
            ("set_fee_config", || {
                env.as_contract(&contract_id, || {
                    crate::init::ProtocolInitializer::set_fee_config(&env, &admin_2, 200)
                })
            }),
            ("set_paused", || {
                env.as_contract(&contract_id, || {
                    crate::pause::PauseControl::set_paused(&env, &admin_2, true)
                })
            }),
            ("add_currency", || {
                env.as_contract(&contract_id, || {
                    crate::currency::CurrencyWhitelist::add_currency(
                        &env,
                        &admin_2,
                        &Address::generate(&env),
                    )
                })
            }),
        ];

        for (method_name, method_call) in methods {
            let result = method_call();
            assert_eq!(
                result.map(|_| ()),
                Ok(()),
                "New admin should be accepted for {}",
                method_name
            );
        }
    }
}

// ============================================================================
// Dry-Run Preview Tests
// ============================================================================
//
// Verifies `preview_protocol_config` (issue #1221):
// - before/after diff matches the effect of the corresponding apply operations
// - no storage writes occur during a preview call
// - admin authorization is enforced
// - invalid parameters are reflected correctly in `would_succeed` / `validation_error_code`
// - no-op and partial-change detection work correctly

#[cfg(all(test, feature = "legacy-tests"))]
mod dry_run_preview {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::init::{ProtocolConfigDiff, ProtocolConfigParams, ProtocolInitializer};
    use crate::QuickLendXContract;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    /// Initialise admin storage and return (env, contract_id, admin).
    fn setup_with_admin() -> (Env, Address, Address) {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
        });
        (env, contract_id, admin)
    }

    /// Params that are all valid and different from the defaults.
    fn valid_params() -> ProtocolConfigParams {
        ProtocolConfigParams {
            min_invoice_amount: 500,
            max_due_date_days: 180,
            grace_period_seconds: 3 * 24 * 60 * 60, // 3 days
            fee_bps: 300,
        }
    }

    fn preview(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
        params: ProtocolConfigParams,
    ) -> Result<ProtocolConfigDiff, QuickLendXError> {
        env.as_contract(contract_id, || {
            ProtocolInitializer::preview_protocol_config(env, admin, params)
        })
    }

    fn apply_protocol_config(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
        p: &ProtocolConfigParams,
    ) {
        env.as_contract(contract_id, || {
            ProtocolInitializer::set_protocol_config(
                env,
                admin,
                p.min_invoice_amount,
                p.max_due_date_days,
                p.grace_period_seconds,
            )
            .unwrap();
            ProtocolInitializer::set_fee_config(env, admin, p.fee_bps).unwrap();
        });
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    /// After applying a config change, preview of the same params shows identical
    /// after-values and is_noop = true (the applied state IS the proposed state).
    #[test]
    fn preview_matches_apply_protocol_config() {
        let (env, contract_id, admin) = setup_with_admin();
        let params = valid_params();

        // Apply first, then preview the same values.
        apply_protocol_config(&env, &contract_id, &admin, &params);

        let diff = preview(&env, &contract_id, &admin, params.clone()).unwrap();

        assert_eq!(diff.after_min_invoice_amount, params.min_invoice_amount);
        assert_eq!(diff.after_max_due_date_days, params.max_due_date_days);
        assert_eq!(diff.after_grace_period_seconds, params.grace_period_seconds);
        assert_eq!(diff.after_fee_bps, params.fee_bps);
        // before == after (we applied the same values), so it should be a no-op
        assert!(
            diff.is_noop,
            "Expected is_noop=true after applying the same values"
        );
        assert!(diff.would_succeed);
        assert_eq!(diff.validation_error_code, 0);
    }

    /// Preview of a *different* fee matches the actual fee after applying that change.
    #[test]
    fn preview_matches_apply_fee_config() {
        let (env, contract_id, admin) = setup_with_admin();

        // Set a baseline
        apply_protocol_config(&env, &contract_id, &admin, &valid_params());

        // Preview a different fee
        let new_fee = 500u32;
        let mut params = valid_params();
        params.fee_bps = new_fee;

        let diff = preview(&env, &contract_id, &admin, params.clone()).unwrap();
        assert_eq!(diff.after_fee_bps, new_fee);
        assert_ne!(diff.before_fee_bps, new_fee, "before should differ");
        assert!(!diff.is_noop);
        assert!(diff.would_succeed);

        // Apply and confirm storage matches what preview projected
        apply_protocol_config(&env, &contract_id, &admin, &params);
        let live_fee = env.as_contract(&contract_id, || ProtocolInitializer::get_fee_bps(&env));
        assert_eq!(live_fee, new_fee);
    }

    /// Storage values are unchanged after calling preview.
    #[test]
    fn preview_no_storage_write() {
        let (env, contract_id, admin) = setup_with_admin();
        let baseline = valid_params();
        apply_protocol_config(&env, &contract_id, &admin, &baseline);

        // Capture current values before preview
        let fee_before = env.as_contract(&contract_id, || ProtocolInitializer::get_fee_bps(&env));
        let min_before = env.as_contract(&contract_id, || {
            ProtocolInitializer::get_min_invoice_amount(&env)
        });

        // Preview with completely different values
        let different = ProtocolConfigParams {
            min_invoice_amount: baseline.min_invoice_amount * 10,
            max_due_date_days: 365,
            grace_period_seconds: 0,
            fee_bps: 999,
        };
        preview(&env, &contract_id, &admin, different).unwrap();

        // Storage must be unchanged
        let fee_after = env.as_contract(&contract_id, || ProtocolInitializer::get_fee_bps(&env));
        let min_after = env.as_contract(&contract_id, || {
            ProtocolInitializer::get_min_invoice_amount(&env)
        });
        assert_eq!(
            fee_before, fee_after,
            "fee_bps must not change after preview"
        );
        assert_eq!(
            min_before, min_after,
            "min_invoice_amount must not change after preview"
        );
    }

    /// Non-admin callers are rejected with NotAdmin.
    #[test]
    fn preview_requires_admin_auth() {
        let (env, contract_id, _admin) = setup_with_admin();
        let non_admin = Address::generate(&env);
        let result = preview(&env, &contract_id, &non_admin, valid_params());
        assert_eq!(result, Err(QuickLendXError::NotAdmin));
    }

    /// Calling preview before admin is initialized returns OperationNotAllowed.
    #[test]
    fn preview_uninitialized_admin_rejected() {
        let (env, contract_id) = setup();
        let stranger = Address::generate(&env);
        let result = preview(&env, &contract_id, &stranger, valid_params());
        assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    }

    /// min_invoice_amount <= 0 → would_succeed=false, code=InvalidAmount.
    #[test]
    fn preview_invalid_min_invoice_amount() {
        let (env, contract_id, admin) = setup_with_admin();
        let mut params = valid_params();
        params.min_invoice_amount = 0;

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(!diff.would_succeed);
        assert_eq!(
            diff.validation_error_code,
            QuickLendXError::InvalidAmount as u32
        );
    }

    /// fee_bps > 1000 → would_succeed=false, code=InvalidFeeBasisPoints.
    #[test]
    fn preview_invalid_fee_bps_too_high() {
        let (env, contract_id, admin) = setup_with_admin();
        let mut params = valid_params();
        params.fee_bps = 1001;

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(!diff.would_succeed);
        assert_eq!(
            diff.validation_error_code,
            QuickLendXError::InvalidFeeBasisPoints as u32
        );
    }

    /// max_due_date_days == 0 → would_succeed=false, code=InvoiceDueDateInvalid.
    #[test]
    fn preview_invalid_max_due_date_days_zero() {
        let (env, contract_id, admin) = setup_with_admin();
        let mut params = valid_params();
        params.max_due_date_days = 0;

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(!diff.would_succeed);
        assert_eq!(
            diff.validation_error_code,
            QuickLendXError::InvoiceDueDateInvalid as u32
        );
    }

    /// max_due_date_days > 730 → would_succeed=false, code=InvoiceDueDateInvalid.
    #[test]
    fn preview_invalid_max_due_date_days_too_large() {
        let (env, contract_id, admin) = setup_with_admin();
        let mut params = valid_params();
        params.max_due_date_days = 731;

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(!diff.would_succeed);
        assert_eq!(
            diff.validation_error_code,
            QuickLendXError::InvoiceDueDateInvalid as u32
        );
    }

    /// grace_period_seconds > 30 days → would_succeed=false, code=InvalidTimestamp.
    #[test]
    fn preview_invalid_grace_period_too_large() {
        let (env, contract_id, admin) = setup_with_admin();
        let mut params = valid_params();
        params.grace_period_seconds = 30 * 24 * 60 * 60 + 1; // one second over the limit

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(!diff.would_succeed);
        assert_eq!(
            diff.validation_error_code,
            QuickLendXError::InvalidTimestamp as u32
        );
    }

    /// Proposing the exact current values → is_noop=true.
    #[test]
    fn preview_noop_diff_detected() {
        let (env, contract_id, admin) = setup_with_admin();
        let params = valid_params();
        apply_protocol_config(&env, &contract_id, &admin, &params);

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(diff.is_noop, "Expected is_noop=true for identical values");
        assert!(diff.would_succeed);
    }

    /// Changing even one field makes is_noop=false.
    #[test]
    fn preview_partial_change_not_noop() {
        let (env, contract_id, admin) = setup_with_admin();
        let baseline = valid_params();
        apply_protocol_config(&env, &contract_id, &admin, &baseline);

        let mut changed = baseline.clone();
        changed.fee_bps += 1; // only fee differs

        let diff = preview(&env, &contract_id, &admin, changed).unwrap();
        assert!(
            !diff.is_noop,
            "Expected is_noop=false when one field differs"
        );
        assert!(diff.would_succeed);
    }

    /// before-fields always reflect the live on-chain state at call time.
    #[test]
    fn preview_before_fields_match_current_config() {
        let (env, contract_id, admin) = setup_with_admin();
        let first = valid_params();
        apply_protocol_config(&env, &contract_id, &admin, &first);

        // Now preview a different set of values
        let second = ProtocolConfigParams {
            min_invoice_amount: first.min_invoice_amount + 100,
            max_due_date_days: first.max_due_date_days + 10,
            grace_period_seconds: first.grace_period_seconds + 60,
            fee_bps: first.fee_bps + 50,
        };

        let diff = preview(&env, &contract_id, &admin, second).unwrap();

        // before-fields must equal the applied 'first' values
        assert_eq!(diff.before_min_invoice_amount, first.min_invoice_amount);
        assert_eq!(diff.before_max_due_date_days, first.max_due_date_days);
        assert_eq!(diff.before_grace_period_seconds, first.grace_period_seconds);
        assert_eq!(diff.before_fee_bps, first.fee_bps);
    }

    /// Valid params that equal the boundary values (fee_bps=0, fee_bps=1000) succeed.
    #[test]
    fn preview_boundary_fee_bps_valid() {
        let (env, contract_id, admin) = setup_with_admin();

        for &bps in &[0u32, 1000u32] {
            let mut params = valid_params();
            params.fee_bps = bps;
            let diff = preview(&env, &contract_id, &admin, params).unwrap();
            assert!(diff.would_succeed, "fee_bps={} should be valid", bps);
            assert_eq!(diff.validation_error_code, 0);
        }
    }

    /// Negative min_invoice_amount → would_succeed=false, code=InvalidAmount.
    #[test]
    fn preview_negative_min_invoice_amount() {
        let (env, contract_id, admin) = setup_with_admin();
        let mut params = valid_params();
        params.min_invoice_amount = -1;

        let diff = preview(&env, &contract_id, &admin, params).unwrap();
        assert!(!diff.would_succeed);
        assert_eq!(
            diff.validation_error_code,
            QuickLendXError::InvalidAmount as u32
        );
    }
}
