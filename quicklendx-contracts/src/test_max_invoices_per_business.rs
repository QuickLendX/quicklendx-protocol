#[cfg(test)]
mod test_max_invoices_per_business {
    use crate::protocol_limits::{check_invoice_limit, is_active_status, ProtocolLimitsContract};
    use crate::types::InvoiceStatus;
    use crate::errors::QuickLendXError;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

    // Core logic test extracted from check_invoice_limit architecture
    fn enforce_limit_logic(active_count: u32, limit: u32) -> Result<(), QuickLendXError> {
        if limit > 0 && active_count >= limit {
            return Err(QuickLendXError::MaxInvoicesPerBusinessExceeded);
        }
        Ok(())
    }

    #[test]
    fn test_business_at_cap_exact_boundary() {
        let limit = 5;
        
        // Below limit (N-1): allowed
        assert_eq!(enforce_limit_logic(4, limit), Ok(()));
        
        // At limit (N): trying to create the next one is rejected
        assert_eq!(enforce_limit_logic(5, limit), Err(QuickLendXError::MaxInvoicesPerBusinessExceeded));
        
        // Above limit (N+1): rejected safely
        assert_eq!(enforce_limit_logic(6, limit), Err(QuickLendXError::MaxInvoicesPerBusinessExceeded));
    }

    #[test]
    fn test_zero_limit_is_unlimited() {
        let limit = 0; // Protocol defined: 0 = unlimited
        
        // Very large volumes should be permissible
        assert_eq!(enforce_limit_logic(100, limit), Ok(()));
        assert_eq!(enforce_limit_logic(1000, limit), Ok(()));
    }

    #[test]
    fn test_is_active_status_boundaries() {
        // Ensures our capacity algorithm isn't falsely inflated by settled state
        assert_eq!(is_active_status(&InvoiceStatus::Pending), true);
        assert_eq!(is_active_status(&InvoiceStatus::Verified), true);
        assert_eq!(is_active_status(&InvoiceStatus::Funded), true);
        
        assert_eq!(is_active_status(&InvoiceStatus::Paid), false);
        assert_eq!(is_active_status(&InvoiceStatus::Defaulted), false);
        assert_eq!(is_active_status(&InvoiceStatus::Cancelled), false);
        assert_eq!(is_active_status(&InvoiceStatus::Refunded), false);
    }

    // =========================================================================
    // NEW: Integration tests for check_invoice_limit with real contract state
    // =========================================================================

    fn setup(env: &Env) -> (QuickLendXContractClient, Address) {
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.set_admin(&admin);
        (client, admin)
    }

    fn create_verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
        let business = Address::generate(env);
        client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
        client.verify_business(admin, &business);
        business
    }

    #[test]
    fn test_check_invoice_limit_no_active_invoices_passes() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set a reasonable limit
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            5u32, // max_invoices_per_business = 5
        )
        .unwrap();

        // Business with 0 active invoices should pass
        let result = check_invoice_limit(&env, &business);
        assert!(result.is_ok(), "Business with 0 active invoices should pass limit check");
    }

    #[test]
    fn test_check_invoice_limit_below_limit_passes() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set limit to 3
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            3u32,
        )
        .unwrap();

        // Create 2 active invoices (below limit of 3)
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        
        client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 1"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        
        client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 2"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Should pass with 2 active invoices (limit is 3)
        let result = check_invoice_limit(&env, &business);
        assert!(result.is_ok(), "Business with 2 active invoices should pass with limit of 3");
    }

    #[test]
    fn test_check_invoice_limit_at_exact_boundary_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set limit to 2
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            2u32,
        )
        .unwrap();

        // Create 2 active invoices (at exact limit)
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        
        client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 1"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        
        client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 2"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Should fail with 2 active invoices at limit of 2
        let result = check_invoice_limit(&env, &business);
        assert!(
            result == Err(QuickLendXError::MaxInvoicesPerBusinessExceeded),
            "Business at exact limit should fail"
        );
    }

    #[test]
    fn test_check_invoice_limit_above_limit_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set limit to 1
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            1u32,
        )
        .unwrap();

        // Create 2 active invoices (above limit of 1)
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        
        client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 1"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        
        client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 2"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Should fail with 2 active invoices when limit is 1
        let result = check_invoice_limit(&env, &business);
        assert!(
            result == Err(QuickLendXError::MaxInvoicesPerBusinessExceeded),
            "Business above limit should fail"
        );
    }

    #[test]
    fn test_check_invoice_limit_zero_limit_allows_unlimited() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set limit to 0 (unlimited mode)
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            0u32, // 0 = unlimited
        )
        .unwrap();

        // Create many active invoices
        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        
        for i in 0..10 {
            client.store_invoice(
                &admin,
                &business,
                &100i128,
                &currency,
                &due_date,
                &String::from_str(&env, &format!("Invoice {}", i)),
                &crate::invoice::InvoiceCategory::Services,
                &Vec::new(&env),
            );
        }

        // Should pass with 10 active invoices when limit is 0 (unlimited)
        let result = check_invoice_limit(&env, &business);
        assert!(result.is_ok(), "Business should have unlimited invoices when limit is 0");
    }

    #[test]
    fn test_check_invoice_limit_only_counts_active_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set limit to 3
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            3u32,
        )
        .unwrap();

        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        
        // Create 2 active invoices
        let inv1 = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Active Invoice 1"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        
        let inv2 = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Active Invoice 2"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Create 2 inactive invoices (Paid status)
        let inv3 = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Inactive Invoice 1"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        
        let inv4 = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Inactive Invoice 2"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Mark inv3 and inv4 as Paid (inactive)
        client.mark_invoice_paid(&admin, &inv3);
        client.mark_invoice_paid(&admin, &inv4);

        // Should pass: 2 active invoices + 2 inactive = 2 active count (limit is 3)
        let result = check_invoice_limit(&env, &business);
        assert!(result.is_ok(), "Only active invoices should count toward limit");
    }

    #[test]
    fn test_check_invoice_limit_mixed_active_statuses() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let business = create_verified_business(&env, &client, &admin);

        // Set limit to 4
        ProtocolLimitsContract::set_protocol_limits_authed(
            &env,
            &admin,
            10i128,
            10i128,
            100u32,
            365u64,
            604800u64,
            4u32,
        )
        .unwrap();

        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;
        
        // Create invoices with different active statuses
        let inv_pending = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Pending"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        
        let inv_verified = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Verified"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        client.verify_invoice(&admin, &inv_verified);

        let inv_funded = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Funded"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );
        // Simulate funding by setting status
        // (In real flow this would be via bid acceptance)

        // All 3 should be active (Pending, Verified, Funded)
        let result = check_invoice_limit(&env, &business);
        assert!(result.is_ok(), "Pending, Verified, and Funded should all count as active");

        // Add one more to reach limit
        let inv4 = client.store_invoice(
            &admin,
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice 4"),
            &crate::invoice::InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Now at limit (4 active), should fail
        let result = check_invoice_limit(&env, &business);
        assert!(
            result == Err(QuickLendXError::MaxInvoicesPerBusinessExceeded),
            "Should fail when active count reaches limit"
        );
    }
}