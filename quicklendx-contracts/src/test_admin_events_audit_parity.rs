//! Events and audit parity regression tests for administrator role management,
//! pause control, and emergency recovery operations (QE-2026-08).

#[cfg(test)]
mod test_admin_events_audit_parity {
    use crate::admin::AdminStorage;
    use crate::audit::{
        AuditOperation, AuditOperationFilter, AuditQueryFilter, AuditStorage, CONFIG_AUDIT_SENTINEL,
    };
    use crate::emergency::EmergencyWithdraw;
    use crate::errors::QuickLendXError;
    use crate::pause::PauseControl;
    use crate::QuickLendXContract;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        xdr, Address, BytesN, Env, Symbol, TryFromVal,
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
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap();
        });
        (env, contract_id, admin)
    }

    fn existing_destination(env: &Env) -> Address {
        env.register(QuickLendXContract, ())
    }

    fn sentinel_bytes(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &CONFIG_AUDIT_SENTINEL)
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

    // ========================================================================
    // Admin Initialization Events & Audit Parity
    // ========================================================================

    #[test]
    fn test_admin_initialize_emits_event_and_audit_entry_on_success() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let res = AdminStorage::initialize(&env, &admin);
            assert_eq!(res, Ok(()));

            // Verify event emission
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "adm_init"));

            // Verify audit entry on CONFIG_AUDIT_SENTINEL
            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminInitialized),
                actor: Some(admin.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            let entries = AuditStorage::query_audit_logs(&env, &filter, 10);
            assert_eq!(entries.len(), 1);
            let entry = entries.get(0).unwrap();
            assert_eq!(entry.operation, AuditOperation::AdminInitialized);
            assert_eq!(entry.actor, admin);

            // Verify hash chain validity
            assert!(AuditStorage::verify_audit_chain(
                &env,
                &sentinel_bytes(&env)
            ));
        });

        // Re-initialization must fail and emit NO second event or audit entry
        let event_count_before = env.events().all().events().len();
        let admin_2 = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let res = AdminStorage::initialize(&env, &admin_2);
            assert_eq!(res, Err(QuickLendXError::OperationNotAllowed));

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminInitialized),
                actor: None,
                start_timestamp: None,
                end_timestamp: None,
            };
            let entries = AuditStorage::query_audit_logs(&env, &filter, 10);
            assert_eq!(entries.len(), 1); // Still only 1 entry for admin
        });
        assert_eq!(env.events().all().events().len(), event_count_before);
    }

    // ========================================================================
    // Admin Direct & Two-Step Transfers Parity
    // ========================================================================

    #[test]
    fn test_admin_transfer_parity_success_and_rejection() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = existing_destination(&env);
        let attacker = Address::generate(&env);

        let event_count_start = env.events().all().events().len();

        // Failed transfer by attacker: no state change, no event, no audit entry
        env.as_contract(&contract_id, || {
            let res = AdminStorage::transfer_admin(&env, &attacker, &admin_2);
            assert_eq!(res, Err(QuickLendXError::NotAdmin));

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminTransferred),
                actor: None,
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(AuditStorage::query_audit_logs(&env, &filter, 10).len(), 0);
        });
        assert_eq!(env.events().all().events().len(), event_count_start);

        // Successful direct transfer
        env.as_contract(&contract_id, || {
            let res = AdminStorage::transfer_admin(&env, &admin_1, &admin_2);
            assert_eq!(res, Ok(()));

            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "adm_trf"));

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminTransferred),
                actor: Some(admin_1.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            let entries = AuditStorage::query_audit_logs(&env, &filter, 10);
            assert_eq!(entries.len(), 1);
            let entry = entries.get(0).unwrap();
            assert_eq!(entry.actor, admin_1);
            assert!(AuditStorage::verify_audit_chain(
                &env,
                &sentinel_bytes(&env)
            ));
        });
    }

    #[test]
    fn test_two_step_transfer_lifecycle_events_and_audit_parity() {
        let (env, contract_id, admin_1) = setup_with_admin();
        let admin_2 = existing_destination(&env);

        env.as_contract(&contract_id, || {
            // Enable two-step mode
            AdminStorage::set_two_step_enabled(&env, &admin_1, true).unwrap();
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "adm_2st"));

            let filter_2st = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminTwoStepUpdated),
                actor: Some(admin_1.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_2st, 10).len(),
                1
            );

            // Initiate two-step transfer
            AdminStorage::transfer_admin(&env, &admin_1, &admin_2).unwrap();
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "adm_req"));

            let filter_init = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminTransferInitiated),
                actor: Some(admin_1.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_init, 10).len(),
                1
            );

            // Cancel two-step transfer
            AdminStorage::cancel_admin_transfer(&env, &admin_1).unwrap();
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "adm_cnl"));

            let filter_cnl = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminTransferCancelled),
                actor: Some(admin_1.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_cnl, 10).len(),
                1
            );

            // Re-initiate and Accept
            AdminStorage::initiate_admin_transfer(&env, &admin_1, &admin_2).unwrap();
            AdminStorage::accept_admin_transfer(&env, &admin_2).unwrap();
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "adm_trf"));

            let filter_acc = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::AdminTransferred),
                actor: Some(admin_2.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_acc, 10).len(),
                1
            );

            // Verify hash chain validity
            assert!(AuditStorage::verify_audit_chain(
                &env,
                &sentinel_bytes(&env)
            ));
        });
    }

    // ========================================================================
    // Pause Controls Events & Audit Parity
    // ========================================================================

    #[test]
    fn test_pause_and_unpause_events_and_audit_parity() {
        let (env, contract_id, admin) = setup_with_admin();
        let attacker = Address::generate(&env);

        // Unauthorized pause attempt: rejected, no state change, no event, no audit
        let events_start = env.events().all().events().len();
        env.as_contract(&contract_id, || {
            let res = PauseControl::set_paused(&env, &attacker, true);
            assert_eq!(res, Err(QuickLendXError::NotAdmin));

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::ProtocolPaused),
                actor: None,
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(AuditStorage::query_audit_logs(&env, &filter, 10).len(), 0);
        });
        assert_eq!(env.events().all().events().len(), events_start);

        // Pause contract successfully
        env.as_contract(&contract_id, || {
            PauseControl::set_paused(&env, &admin, true).unwrap();
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "Paused"));

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::ProtocolPaused),
                actor: Some(admin.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(AuditStorage::query_audit_logs(&env, &filter, 10).len(), 1);

            // Repeated pause (no-op): returns Ok(()), no new event or audit
            let ev_len = env.events().all().events().len();
            PauseControl::set_paused(&env, &admin, true).unwrap();
            assert_eq!(env.events().all().events().len(), ev_len);
            assert_eq!(AuditStorage::query_audit_logs(&env, &filter, 10).len(), 1);

            // Unpause contract successfully
            PauseControl::set_paused(&env, &admin, false).unwrap();
            assert_eq!(latest_topic_symbol(&env), Symbol::new(&env, "Unpaused"));

            let filter_unp = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(AuditOperation::ProtocolUnpaused),
                actor: Some(admin.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_unp, 10).len(),
                1
            );

            assert!(AuditStorage::verify_audit_chain(
                &env,
                &sentinel_bytes(&env)
            ));
        });
    }

    // ========================================================================
    // Emergency Recovery Events & Audit Parity
    // ========================================================================

    #[test]
    fn test_emergency_withdraw_initiate_and_cancel_parity() {
        let (env, contract_id, admin) = setup_with_admin();
        let token = Address::generate(&env);
        let target = Address::generate(&env);
        let attacker = Address::generate(&env);

        // Unauthorized initiate: fails, no event, no audit
        let ev_start = env.events().all().events().len();
        env.as_contract(&contract_id, || {
            let res =
                EmergencyWithdraw::initiate(&env, &attacker, token.clone(), 100, target.clone());
            assert_eq!(res, Err(QuickLendXError::NotAdmin));

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(
                    AuditOperation::EmergencyWithdrawalInitiated,
                ),
                actor: None,
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(AuditStorage::query_audit_logs(&env, &filter, 10).len(), 0);
        });
        assert_eq!(env.events().all().events().len(), ev_start);

        // Authorized initiate
        env.as_contract(&contract_id, || {
            EmergencyWithdraw::initiate(&env, &admin, token.clone(), 100, target.clone()).unwrap();

            let filter = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(
                    AuditOperation::EmergencyWithdrawalInitiated,
                ),
                actor: Some(admin.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            let entries = AuditStorage::query_audit_logs(&env, &filter, 10);
            assert_eq!(entries.len(), 1);
            let entry = entries.get(0).unwrap();
            assert_eq!(entry.amount, Some(100));

            // Authorized cancel
            EmergencyWithdraw::cancel(&env, &admin).unwrap();

            let filter_cnl = AuditQueryFilter {
                invoice_id: Some(sentinel_bytes(&env)),
                operation: AuditOperationFilter::Specific(
                    AuditOperation::EmergencyWithdrawalCancelled,
                ),
                actor: Some(admin.clone()),
                start_timestamp: None,
                end_timestamp: None,
            };
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_cnl, 10).len(),
                1
            );

            // Re-cancel fails, emits no duplicate event or audit entry
            let ev_mid = env.events().all().events().len();
            let res = EmergencyWithdraw::cancel(&env, &admin);
            assert_eq!(res, Err(QuickLendXError::EmergencyWithdrawCancelled));
            assert_eq!(env.events().all().events().len(), ev_mid);
            assert_eq!(
                AuditStorage::query_audit_logs(&env, &filter_cnl, 10).len(),
                1
            );

            assert!(AuditStorage::verify_audit_chain(
                &env,
                &sentinel_bytes(&env)
            ));
        });
    }
}
