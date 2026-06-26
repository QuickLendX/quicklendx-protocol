#[cfg(test)]
mod test_protocol_limits_boundary {
    use crate::admin::AdminStorage;
    use crate::errors::QuickLendXError;
    use crate::protocol_limits::{ProtocolLimits, ProtocolLimitsContract};
    use crate::QuickLendXContract;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env,
    };

    fn setup_with_admin() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            AdminStorage::initialize(&env, &admin).unwrap()
        });
        env.ledger().set_timestamp(1_000_000);
        (env, contract_id, admin)
    }

    fn setup_env_with_limits() -> (Env, ProtocolLimits) {
        let env = Env::default();
        env.mock_all_auths();
        let limits = ProtocolLimits {
            min_invoice_amount: 1_000_000,
            min_bid_amount: 10,
            min_bid_bps: 100,
            max_due_date_days: 90,
            grace_period_seconds: 86_400,
            max_invoices_per_business: 5,
        };
        env.storage().instance().set(&"protocol_limits", &limits);
        env.ledger().set_timestamp(1_000_000);
        (env, limits)
    }

    #[test]
    fn test_min_invoice_amount_exact_boundary() {
        let (env, limits) = setup_env_with_limits();
        let valid_due_date = env.ledger().timestamp() + (10 * 86400);

        assert!(ProtocolLimitsContract::validate_invoice(
            env.clone(),
            limits.min_invoice_amount,
            valid_due_date
        )
        .is_ok());
        assert_eq!(
            ProtocolLimitsContract::validate_invoice(
                env.clone(),
                limits.min_invoice_amount - 1,
                valid_due_date
            ),
            Err(QuickLendXError::InvalidAmount)
        );
    }

    #[test]
    fn test_max_due_date_days_exact_boundary() {
        let (env, limits) = setup_env_with_limits();
        let valid_amount = limits.min_invoice_amount * 2;
        let max_due_date = env.ledger().timestamp() + (limits.max_due_date_days * 86400);

        assert!(
            ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount, max_due_date)
                .is_ok()
        );
        assert_eq!(
            ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount, max_due_date + 1),
            Err(QuickLendXError::InvoiceDueDateInvalid)
        );
    }

    #[test]
    fn test_set_protocol_config_atomic_application() {
        // set_protocol_limits rejects invalid min_invoice_amount before any storage write,
        // exercising the same validation as the former private validate_protocol_limits_params.
        let (env, contract_id, admin) = setup_with_admin();
        let result = env.as_contract(&contract_id, || {
            ProtocolLimitsContract::set_protocol_limits(
                env.clone(),
                admin.clone(),
                0, // invalid: must be > 0
                10,
                100,
                90,
                86_400,
                5,
            )
        });
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    }
}
