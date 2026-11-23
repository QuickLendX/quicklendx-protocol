//! Contract-level tests for the initialization and admin entrypoints (#2925).
//!
//! These go through `QuickLendXContractClient`, so they cover the public
//! contract surface (argument encoding, auth, error mapping), not only the
//! module functions underneath.

#[cfg(test)]
mod test_contract_init_admin {
    use crate::admin::{ADMIN_INITIALIZED_KEY, ADMIN_KEY};
    use crate::errors::QuickLendXError;
    use crate::init::InitializationParams;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, AuthorizedFunction, MockAuth, MockAuthInvoke},
        Address, Env, IntoVal, Symbol, Vec,
    };

    fn params(env: &Env, admin: &Address) -> InitializationParams {
        InitializationParams {
            admin: admin.clone(),
            treasury: Address::generate(env),
            fee_bps: 200,
            min_invoice_amount: 1_000_000,
            max_due_date_days: 365,
            grace_period_seconds: 604_800,
            initial_currencies: Vec::new(env),
            corridors: Vec::new(env),
            backfill_max_batch_size: 100,
        }
    }

    fn register(env: &Env) -> QuickLendXContractClient<'static> {
        let id = env.register(QuickLendXContract, ());
        QuickLendXContractClient::new(env, &id)
    }

    /// Number of top-level authorizations `addr` gave in the last invocation.
    fn auth_count(env: &Env, addr: &Address) -> usize {
        env.auths().iter().filter(|(a, _)| a == addr).count()
    }

    /// Simulate a deployment whose admin was installed by a legacy path
    /// (the removed `initialize_admin`): admin set, protocol not initialized.
    fn preset_legacy_admin(env: &Env, client: &QuickLendXContractClient, admin: &Address) {
        env.as_contract(&client.address, || {
            env.storage().instance().set(&ADMIN_KEY, admin);
            env.storage().instance().set(&ADMIN_INITIALIZED_KEY, &true);
        });
    }

    #[test]
    fn initialize_requires_admin_auth() {
        let env = Env::default();
        let client = register(&env);
        let admin = Address::generate(&env);
        let p = params(&env, &admin);

        // No auth mocked at all: the admin has not signed.
        let r = client.try_initialize(&p);
        assert!(
            matches!(r, Err(Err(_))),
            "unsigned initialize must fail with an auth error, got {:?}",
            r
        );
        assert!(!client.is_initialized());
        assert_eq!(client.get_current_admin(), None);

        // Someone else signing does not help either.
        let other = Address::generate(&env);
        let r = client
            .mock_auths(&[MockAuth {
                address: &other,
                invoke: &MockAuthInvoke {
                    contract: &client.address,
                    fn_name: "initialize",
                    args: (p.clone(),).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .try_initialize(&p);
        assert!(
            matches!(r, Err(Err(_))),
            "initialize signed by a non-admin must fail, got {:?}",
            r
        );
        assert!(!client.is_initialized());

        // With exactly the admin's signature for this call it succeeds, and the
        // admin is asked to authorize exactly once.
        client
            .mock_auths(&[MockAuth {
                address: &admin,
                invoke: &MockAuthInvoke {
                    contract: &client.address,
                    fn_name: "initialize",
                    args: (p.clone(),).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .initialize(&p);
        assert_eq!(auth_count(&env, &admin), 1);
        assert_eq!(
            env.auths()[0].1.function,
            AuthorizedFunction::Contract((
                client.address.clone(),
                Symbol::new(&env, "initialize"),
                (p.clone(),).into_val(&env),
            ))
        );
        assert!(client.is_initialized());
        assert_eq!(client.get_current_admin(), Some(admin));
    }

    #[test]
    fn initialize_is_idempotent_for_identical_params() {
        let env = Env::default();
        env.mock_all_auths();
        let client = register(&env);
        let admin = Address::generate(&env);
        let p = params(&env, &admin);

        client.initialize(&p);
        assert_eq!(client.try_initialize(&p), Ok(Ok(())));
        assert_eq!(client.get_fee_bps(), 200);

        let mut changed = p.clone();
        changed.fee_bps = 201;
        assert_eq!(
            client.try_initialize(&changed),
            Err(Ok(QuickLendXError::OperationNotAllowed))
        );
        assert_eq!(
            client.get_fee_bps(),
            200,
            "rejected re-init must not change state"
        );
    }

    #[test]
    fn legacy_admin_then_initialize_succeeds() {
        // Admin A pre-installed by a legacy path; A completes initialization.
        let env = Env::default();
        env.mock_all_auths();
        let client = register(&env);
        let a = Address::generate(&env);
        preset_legacy_admin(&env, &client, &a);
        assert!(
            !client.is_initialized(),
            "a legacy admin alone must not count as initialized"
        );

        let p = params(&env, &a);
        assert_eq!(client.try_initialize(&p), Ok(Ok(())));
        assert_eq!(auth_count(&env, &a), 1, "A must authorize exactly once");
        assert!(client.is_initialized());
        assert_eq!(client.get_treasury(), Some(p.treasury.clone()));
        assert_eq!(client.get_fee_bps(), 200);
        assert_eq!(client.get_current_admin(), Some(a.clone()));

        // Same pre-set state, but B tries to initialize: rejected, nothing written.
        let env = Env::default();
        env.mock_all_auths();
        let client = register(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        preset_legacy_admin(&env, &client, &a);

        let p = params(&env, &b);
        assert_eq!(
            client.try_initialize(&p),
            Err(Ok(QuickLendXError::OperationNotAllowed))
        );
        assert!(!client.is_initialized());
        assert_eq!(client.get_treasury(), None);
        assert_eq!(client.get_current_admin(), Some(a));
    }

    #[test]
    fn two_step_transfer_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let client = register(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        client.initialize(&params(&env, &a));

        client.set_two_step_enabled(&a, &true);
        assert!(client.is_two_step_enabled());

        client.transfer_admin(&a, &b);
        assert_eq!(client.get_current_admin(), Some(a.clone()));
        assert_eq!(client.get_pending_admin(), Some(b.clone()));

        assert_eq!(
            client.try_accept_admin_transfer(&c),
            Err(Ok(QuickLendXError::Unauthorized))
        );
        assert_eq!(client.get_current_admin(), Some(a.clone()));
        assert_eq!(client.get_pending_admin(), Some(b.clone()));

        client.accept_admin_transfer(&b);
        assert_eq!(client.get_current_admin(), Some(b.clone()));
        assert_eq!(client.get_pending_admin(), None);

        // The old admin has lost admin rights on the contract surface.
        assert_eq!(
            client.try_set_fee_config(&a, &300u32),
            Err(Ok(QuickLendXError::NotAdmin))
        );
        client.set_fee_config(&b, &300u32);
        assert_eq!(client.get_fee_bps(), 300);
    }

    #[test]
    fn admin_entrypoints_require_admin_signature() {
        let env = Env::default();
        let client = register(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&params(&env, &a));

        // From here on nothing is signed.
        env.set_auths(&[]);
        assert!(matches!(
            client.try_set_fee_config(&a, &300u32),
            Err(Err(_))
        ));
        assert!(matches!(client.try_set_treasury(&a, &b), Err(Err(_))));
        assert!(matches!(
            client.try_set_protocol_config(&a, &2_000_000i128, &180u64, &86_400u64, &100u32),
            Err(Err(_))
        ));
        assert!(matches!(client.try_transfer_admin(&a, &b), Err(Err(_))));
        assert!(matches!(
            client.try_initiate_admin_transfer(&a, &b),
            Err(Err(_))
        ));
        assert!(matches!(client.try_cancel_admin_transfer(&a), Err(Err(_))));
        assert!(matches!(
            client.try_set_two_step_enabled(&a, &true),
            Err(Err(_))
        ));
        assert!(matches!(client.try_accept_admin_transfer(&b), Err(Err(_))));
        assert_eq!(client.get_fee_bps(), 200);
        assert_eq!(client.get_current_admin(), Some(a));
    }
}
