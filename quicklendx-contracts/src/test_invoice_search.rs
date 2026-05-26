#[cfg(test)]
mod test_invoice_search {
    use super::*;
    use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
    use crate::types::{SearchRank, SearchResult};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String, Vec};

    fn setup_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn create_test_invoice(
        env: &Env,
        business: &Address,
        description: &str,
        customer_name: Option<&str>,
        id_override: Option<&str>,
    ) -> Invoice {
        let id = if let Some(id_str) = id_override {
            // Create a deterministic ID for testing
            let mut bytes = [0u8; 32];
            let id_bytes = id_str.as_bytes();
            for (i, &b) in id_bytes.iter().enumerate().take(32) {
                bytes[i] = b;
            }
            BytesN::from_array(env, &bytes)
        } else {
            env.crypto().sha256(&env.crypto().random_bytes())
        };

        let mut invoice = Invoice {
            id,
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(env),
            due_date: env.ledger().timestamp() + 86400,
            status: InvoiceStatus::Verified,
            created_at: env.ledger().timestamp(),
            description: String::from_str(env, description),
            metadata_customer_name: customer_name.map(|name| String::from_str(env, name)),
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(env),
            category: InvoiceCategory::Services,
            tags: Vec::new(env),
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(env),
            dispute_status: crate::invoice::DisputeStatus::None,
            dispute: Default::default(),
            total_paid: 0,
            payment_history: Vec::new(env),
        };
        invoice
    }

    #[test]
    fn test_search_invoices_exact_id_match() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoice with known ID
        let test_id = "test_invoice_123";
        let invoice = create_test_invoice(&env, &business, "consulting services", Some("ABC Corp"), Some(test_id));

        // Mock storage - store the invoice
        InvoiceStorage::store_invoice(&env, &invoice);

        // Search for the exact ID
        let query = String::from_str(&env, test_id);
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0).unwrap().rank, SearchRank::ExactId);
        assert_eq!(results.get(0).unwrap().invoice_id, invoice.id);
    }

    #[test]
    fn test_search_invoices_partial_description_match() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoices
        let invoice1 = create_test_invoice(&env, &business, "software development services", Some("XYZ Ltd"), None);
        let invoice2 = create_test_invoice(&env, &business, "marketing campaign", Some("ABC Corp"), None);

        // Store invoices
        InvoiceStorage::store_invoice(&env, &invoice1);
        InvoiceStorage::store_invoice(&env, &invoice2);

        // Search for "software"
        let query = String::from_str(&env, "software");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0).unwrap().rank, SearchRank::PartialMatch);
        assert_eq!(results.get(0).unwrap().invoice_id, invoice1.id);
    }

    #[test]
    fn test_search_invoices_customer_name_match() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoices
        let invoice1 = create_test_invoice(&env, &business, "consulting", Some("ABC Corporation"), None);
        let invoice2 = create_test_invoice(&env, &business, "development", Some("XYZ Ltd"), None);

        // Store invoices
        InvoiceStorage::store_invoice(&env, &invoice1);
        InvoiceStorage::store_invoice(&env, &invoice2);

        // Search for "ABC"
        let query = String::from_str(&env, "abc");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0).unwrap().rank, SearchRank::PartialMatch);
        assert_eq!(results.get(0).unwrap().invoice_id, invoice1.id);
    }

    #[test]
    fn test_search_invoices_multiple_results_ranking() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoice with exact ID match
        let test_id = "exact_match_123";
        let invoice_exact = create_test_invoice(&env, &business, "general services", Some("Test Corp"), Some(test_id));

        // Create invoice with partial match
        let invoice_partial = create_test_invoice(&env, &business, "software development", Some("Other Corp"), None);

        // Store invoices
        InvoiceStorage::store_invoice(&env, &invoice_exact);
        InvoiceStorage::store_invoice(&env, &invoice_partial);

        // Search for "exact" (should match both, but exact ID ranks higher)
        let query = String::from_str(&env, "exact");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 2);

        // First result should be exact ID match
        let first = results.get(0).unwrap();
        assert_eq!(first.rank, SearchRank::ExactId);
        assert_eq!(first.invoice_id, invoice_exact.id);

        // Second result should be partial match
        let second = results.get(1).unwrap();
        assert_eq!(second.rank, SearchRank::PartialMatch);
        assert_eq!(second.invoice_id, invoice_partial.id);
    }

    #[test]
    fn test_search_invoices_no_matches() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoice
        let invoice = create_test_invoice(&env, &business, "consulting services", Some("ABC Corp"), None);
        InvoiceStorage::store_invoice(&env, &invoice);

        // Search for non-existent term
        let query = String::from_str(&env, "nonexistent");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_invoices_case_insensitive() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoice with uppercase
        let invoice = create_test_invoice(&env, &business, "SOFTWARE DEVELOPMENT", Some("ABC CORP"), None);
        InvoiceStorage::store_invoice(&env, &invoice);

        // Search with lowercase
        let query = String::from_str(&env, "software");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0).unwrap().rank, SearchRank::PartialMatch);
    }

    #[test]
    fn test_search_invoices_input_sanitization() {
        let env = setup_test_env();

        // Test empty query
        let empty_query = String::from_str(&env, "");
        assert!(InvoiceSearch::sanitize_query(&empty_query).is_err());

        // Test query with only spaces
        let spaces_query = String::from_str(&env, "   ");
        assert!(InvoiceSearch::sanitize_query(&spaces_query).is_err());

        // Test too long query
        let long_query = String::from_str(&env, &"a".repeat(101));
        assert!(InvoiceSearch::sanitize_query(&long_query).is_err());

        // Test valid query with punctuation (should be removed)
        let punctuated_query = String::from_str(&env, "test-query!");
        let sanitized = InvoiceSearch::sanitize_query(&punctuated_query).unwrap();
        assert_eq!(sanitized, String::from_str(&env, "testquery"));
    }

    #[test]
    fn test_search_invoices_result_limit() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create more than MAX_SEARCH_RESULTS invoices
        for i in 0..60 {
            let description = format!("service {}", i);
            let invoice = create_test_invoice(&env, &business, &description, None, None);
            InvoiceStorage::store_invoice(&env, &invoice);
        }

        // Search for "service" (should match all)
        let query = String::from_str(&env, "service");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        // Should be limited to MAX_SEARCH_RESULTS
        assert_eq!(results.len() as u32, MAX_SEARCH_RESULTS);
    }

    #[test]
    fn test_search_invoices_relevance_ordering() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create invoices at different times
        let mut invoice1 = create_test_invoice(&env, &business, "old service", Some("Test Corp"), None);
        invoice1.created_at = 1000;

        let mut invoice2 = create_test_invoice(&env, &business, "new service", Some("Test Corp"), None);
        invoice2.created_at = 2000;

        // Store invoices
        InvoiceStorage::store_invoice(&env, &invoice1);
        InvoiceStorage::store_invoice(&env, &invoice2);

        // Search for "service"
        let query = String::from_str(&env, "service");
        let results = InvoiceSearch::search_invoices(&env, query).unwrap();

        assert_eq!(results.len(), 2);

        // Should be ordered by created_at descending (newest first)
        assert_eq!(results.get(0).unwrap().created_at, 2000);
        assert_eq!(results.get(1).unwrap().created_at, 1000);
    }
}