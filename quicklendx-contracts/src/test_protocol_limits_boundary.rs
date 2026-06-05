#[cfg(test)]
mod test_protocol_limits_boundary {
    use soroban_sdk::{testutils::Ledger, Env};
    use crate::protocol_limits::{ProtocolLimitsContract, ProtocolLimits, LIMITS_KEY, validate_protocol_limits_params};
    use crate::errors::QuickLendXError;

    fn setup_env_with_limits() -> (Env, ProtocolLimits) {
        let env = Env::default();
        let limits = ProtocolLimits {
            min_invoice_amount: 1_000_000,
            min_bid_amount: 10,
            min_bid_bps: 100,
            max_due_date_days: 90,
            grace_period_seconds: 86_400,
            max_invoices_per_business: 5,
        };
        env.storage().instance().set(&LIMITS_KEY, &limits);
        env.ledger().set_timestamp(1_000_000);
        (env, limits)
    }

    #[test]
    fn test_min_invoice_amount_exact_boundary() {
        let (env, limits) = setup_env_with_limits();
        let valid_due_date = env.ledger().timestamp() + (10 * 86400); 
        
        // Boundary: exactly min_invoice_amount is allowed
        assert!(ProtocolLimitsContract::validate_invoice(env.clone(), limits.min_invoice_amount, valid_due_date).is_ok());
        
        // Boundary: min_invoice_amount - 1 is rejected (off-by-one check)
        assert_eq!(
            ProtocolLimitsContract::validate_invoice(env.clone(), limits.min_invoice_amount - 1, valid_due_date),
            Err(QuickLendXError::InvalidAmount)
        );
    }

    #[test]
    fn test_max_due_date_days_exact_boundary() {
        let (env, limits) = setup_env_with_limits();
        let valid_amount = limits.min_invoice_amount * 2;
        
        // Boundary: exactly max_due_date is allowed
        let max_due_date = env.ledger().timestamp() + (limits.max_due_date_days * 86400);
        assert!(ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount, max_due_date).is_ok());
        
        // Boundary: max_due_date + 1 second is rejected (off-by-one check)
        assert_eq!(
            ProtocolLimitsContract::validate_invoice(env.clone(), valid_amount, max_due_date + 1),
            Err(QuickLendXError::InvoiceDueDateInvalid)
        );
    }

    #[test]
    fn test_protocol_limits_atomicity_validation() {
        // Validate params rejects bad input BEFORE any storage writes can occur
        let result = validate_protocol_limits_params(
            0, // invalid min amount (<= 0)
            10, 100, 90, 86400
        );
        assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    }
}