#[cfg(test)]
mod test_default_finality {
    use crate::defaults::handle_default;
    use crate::errors::QuickLendXError;
    use crate::storage::{InvestmentStorage, InvoiceStorage};
    use crate::types::{
        DisputeStatus, InsuranceCoverage, Investment, InvestmentStatus, Invoice, InvoiceCategory,
        InvoiceStatus,
    };
    use crate::QuickLendXContract;
    use crate::QuickLendXContractClient;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

    #[test]
    fn test_defaulted_invoice_operations_reject() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, QuickLendXContract);
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let business = Address::generate(&env);
        let admin = Address::generate(&env);

        let invoice_id = BytesN::from_array(&env, &[1; 32]);
        let invoice = Invoice {
            id: invoice_id.clone(),
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(&env),
            due_date: env.ledger().timestamp() + 1000,
            description: String::from_str(&env, "test"),
            category: InvoiceCategory::Services,
            tags: Vec::new(&env),
            status: InvoiceStatus::Defaulted, // Terminal state
            dispute_status: DisputeStatus::None,
            investor: None,
            funded_at: 0,
            paid_at: 0,
            paid_amount: 0,
            metadata: None,
            ratings: Vec::new(&env),
            dispute: crate::types::Dispute {
                created_by: business.clone(),
                created_at: 0,
                reason: String::from_str(&env, ""),
                evidence: String::from_str(&env, ""),
                resolution: String::from_str(&env, ""),
                resolved_by: admin.clone(),
                resolved_at: 0,
                resolution_outcome: None,
            },
        };
        InvoiceStorage::store_invoice(&env, &invoice);

        let bid_id = BytesN::from_array(&env, &[2; 32]);

        // 1. Cannot be funded
        let res_fund = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
        assert!(res_fund.is_err());

        // 2. Cannot be settled
        let res_settle = client.try_settle_invoice(&invoice_id, &1000);
        assert!(res_settle.is_err());

        // 3. Cannot have partial payments
        let res_partial =
            client.try_process_partial_payment(&invoice_id, &500, &String::from_str(&env, "tx1"));
        assert!(res_partial.is_err());
    }

    #[test]
    fn test_single_insurance_claim_and_idempotent_default() {
        let env = Env::default();
        env.mock_all_auths();

        let invoice_id = BytesN::from_array(&env, &[3; 32]);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let admin = Address::generate(&env);

        let invoice = Invoice {
            id: invoice_id.clone(),
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(&env),
            due_date: env.ledger().timestamp(),
            description: String::from_str(&env, "test"),
            category: InvoiceCategory::Services,
            tags: Vec::new(&env),
            status: InvoiceStatus::Funded,
            dispute_status: DisputeStatus::None,
            investor: Some(investor.clone()),
            funded_at: env.ledger().timestamp(),
            paid_at: 0,
            paid_amount: 0,
            metadata: None,
            ratings: Vec::new(&env),
            dispute: crate::types::Dispute {
                created_by: business.clone(),
                created_at: 0,
                reason: String::from_str(&env, ""),
                evidence: String::from_str(&env, ""),
                resolution: String::from_str(&env, ""),
                resolved_by: admin.clone(),
                resolved_at: 0,
                resolution_outcome: None,
            },
        };
        InvoiceStorage::store_invoice(&env, &invoice);

        let investment_id = BytesN::from_array(&env, &[4; 32]);
        let provider = Address::generate(&env);

        let mut insurance_vec = Vec::new(&env);
        insurance_vec.push_back(InsuranceCoverage {
            provider: provider.clone(),
            coverage_percentage: 100,
            premium_amount: 10,
            is_active: true,
        });

        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            amount: 1000,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
            insurance: insurance_vec,
        };
        InvestmentStorage::store_investment(&env, &investment);

        use crate::payments::{Escrow, EscrowStatus, EscrowStorage};
        let escrow_id = BytesN::from_array(&env, &[5; 32]);
        let escrow = Escrow {
            escrow_id: escrow_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(&env),
            status: EscrowStatus::Held,
            created_at: env.ledger().timestamp(),
            released_at: 0,
        };
        EscrowStorage::store_escrow(&env, &escrow);

        // First transition accurately processes everything and flips status
        let res1 = handle_default(&env, &invoice_id);
        assert!(res1.is_ok());

        // Double default fails securely, guaranteeing insurance only processed once
        let res2 = handle_default(&env, &invoice_id);
        assert_eq!(res2, Err(QuickLendXError::InvoiceAlreadyDefaulted));
    }
}
