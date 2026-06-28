//! Search ranking relevance and stability tests for QuickLendX invoice search.

#[cfg(test)]
mod test_invoice_search_ranking {
    use crate::invoice_search::InvoiceSearch;
    use crate::storage::InvoiceStorage;
    use crate::types::{
        Dispute, DisputeResolution, Invoice, InvoiceCategory, InvoiceStatus, SearchRank,
        SearchResult,
    };
    use crate::QuickLendXContract;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, BytesN, Env, String, Vec};

    fn setup_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn with_contract<T>(env: &Env, f: impl FnOnce() -> T) -> T {
        let contract_id = env.register(QuickLendXContract, ());
        env.as_contract(&contract_id, f)
    }

    fn create_test_invoice(
        env: &Env,
        business: &Address,
        description: &str,
        customer_name: Option<&str>,
        id_override: Option<&str>,
        created_at: u64,
    ) -> Invoice {
        let id = if let Some(id_str) = id_override {
            let mut bytes = [0u8; 32];
            let id_bytes = id_str.as_bytes();
            for (i, &b) in id_bytes.iter().enumerate().take(32) {
                bytes[i] = b;
            }
            BytesN::from_array(env, &bytes)
        } else {
            let mut bytes = [0u8; 32];
            let time_bytes = created_at.to_be_bytes();
            bytes[0..8].copy_from_slice(&time_bytes);
            // Add some unique description-based bytes to avoid collisions in loop
            let desc_bytes = description.as_bytes();
            for (i, &b) in desc_bytes.iter().enumerate().take(24) {
                bytes[8 + i] = b;
            }
            BytesN::from_array(env, &bytes)
        };

        let dispute = Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        };

        Invoice {
            id,
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(env),
            due_date: env.ledger().timestamp() + 86400,
            status: InvoiceStatus::Verified,
            created_at,
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
            dispute_status: crate::types::DisputeStatus::None,
            dispute,
            total_paid: 0,
            payment_history: Vec::new(env),
        }
    }

    /// Asserts the rank ordering ExactId > PartialMatch.
    /// Non-matching search queries (Other) must not be returned in the results.
    #[test]
    fn test_rank_ordering_tier_precedence() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        let mut exact_id_bytes = [0u8; 32];
        exact_id_bytes[0] = 0x61;
        exact_id_bytes[1] = 0x62;
        exact_id_bytes[2] = 0x63;
        let exact_id = BytesN::from_array(&env, &exact_id_bytes);
        let exact_id_query = "6162630000000000000000000000000000000000000000000000000000000000";

        let dispute = Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(&env, ""),
            evidence: String::from_str(&env, ""),
            resolution: String::from_str(&env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        };

        // Invoice 1: Exact ID match
        let invoice_exact = Invoice {
            id: exact_id.clone(),
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(&env),
            due_date: env.ledger().timestamp() + 86400,
            status: InvoiceStatus::Verified,
            created_at: 1000,
            description: String::from_str(&env, "some random text"),
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(&env),
            category: InvoiceCategory::Services,
            tags: Vec::new(&env),
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(&env),
            dispute_status: crate::types::DisputeStatus::None,
            dispute,
            total_paid: 0,
            payment_history: Vec::new(&env),
        };

        // Invoice 2: Partial description match
        let invoice_partial = create_test_invoice(
            &env,
            &business,
            "this has 6162630000000000000000000000000000000000000000000000000000000000 in description",
            None,
            None,
            1000,
        );

        // Invoice 3: No match
        let invoice_other =
            create_test_invoice(&env, &business, "unrelated search target", None, None, 1000);

        // Query "abc" (corresponds to exact_id hex string)
        let query = String::from_str(&env, exact_id_query);
        let results = with_contract(&env, || {
            InvoiceStorage::store_invoice(&env, &invoice_exact);
            InvoiceStorage::store_invoice(&env, &invoice_partial);
            InvoiceStorage::store_invoice(&env, &invoice_other);
            InvoiceSearch::search_invoices(&env, query).unwrap()
        });

        // Should return 2 results: exact match first, then partial match. Other should be filtered out.
        assert_eq!(results.len(), 2);

        let result_1 = results.get(0).unwrap();
        assert_eq!(result_1.invoice_id, invoice_exact.id);
        assert_eq!(result_1.rank, SearchRank::ExactId);

        let result_2 = results.get(1).unwrap();
        assert_eq!(result_2.invoice_id, invoice_partial.id);
        assert_eq!(result_2.rank, SearchRank::PartialMatch);
    }

    /// Asserts that the secondary sort (by created_at) is stable and deterministic (descending).
    #[test]
    fn test_secondary_sort_stability() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create three invoices with different created_at timestamps
        let invoice_old = create_test_invoice(&env, &business, "abc old", None, None, 1000);
        let invoice_mid = create_test_invoice(&env, &business, "abc mid", None, None, 2000);
        let invoice_new = create_test_invoice(&env, &business, "abc new", None, None, 3000);

        let query = String::from_str(&env, "abc");
        let results = with_contract(&env, || {
            InvoiceStorage::store_invoice(&env, &invoice_old);
            InvoiceStorage::store_invoice(&env, &invoice_mid);
            InvoiceStorage::store_invoice(&env, &invoice_new);
            InvoiceSearch::search_invoices(&env, query).unwrap()
        });

        assert_eq!(results.len(), 3);

        // Sorting must be descending by created_at: new (3000) -> mid (2000) -> old (1000)
        assert_eq!(results.get(0).unwrap().invoice_id, invoice_new.id);
        assert_eq!(results.get(0).unwrap().created_at, 3000);

        assert_eq!(results.get(1).unwrap().invoice_id, invoice_mid.id);
        assert_eq!(results.get(1).unwrap().created_at, 2000);

        assert_eq!(results.get(2).unwrap().invoice_id, invoice_old.id);
        assert_eq!(results.get(2).unwrap().created_at, 1000);
    }

    /// Asserts that an exact-ID query returns the exact match as the first element even if partial matches have newer timestamps.
    #[test]
    fn test_exact_id_surfaces_first() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        let mut exact_id_bytes = [0u8; 32];
        exact_id_bytes[0] = 0x78;
        exact_id_bytes[1] = 0x79;
        exact_id_bytes[2] = 0x7a;
        let exact_id = BytesN::from_array(&env, &exact_id_bytes);
        let exact_id_query = "78797a0000000000000000000000000000000000000000000000000000000000";

        let dispute = Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(&env, ""),
            evidence: String::from_str(&env, ""),
            resolution: String::from_str(&env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        };

        // Invoice with Exact ID match, created at 1000
        let invoice_exact = Invoice {
            id: exact_id.clone(),
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(&env),
            due_date: env.ledger().timestamp() + 86400,
            status: InvoiceStatus::Verified,
            created_at: 1000,
            description: String::from_str(&env, "some description"),
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(&env),
            category: InvoiceCategory::Services,
            tags: Vec::new(&env),
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(&env),
            dispute_status: crate::types::DisputeStatus::None,
            dispute,
            total_paid: 0,
            payment_history: Vec::new(&env),
        };

        // Invoice with PartialMatch, created at 5000 (newer)
        let invoice_partial = create_test_invoice(
            &env,
            &business,
            "this has 78797a0000000000000000000000000000000000000000000000000000000000 in description",
            None,
            None,
            5000,
        );

        // Query by the hex string of exact_id
        let query = String::from_str(&env, exact_id_query);
        let results = with_contract(&env, || {
            InvoiceStorage::store_invoice(&env, &invoice_exact);
            InvoiceStorage::store_invoice(&env, &invoice_partial);
            InvoiceSearch::search_invoices(&env, query).unwrap()
        });

        assert_eq!(results.len(), 2);
        // Exact ID must be first, despite having a lower created_at timestamp
        assert_eq!(results.get(0).unwrap().invoice_id, invoice_exact.id);
        assert_eq!(results.get(0).unwrap().rank, SearchRank::ExactId);

        assert_eq!(results.get(1).unwrap().invoice_id, invoice_partial.id);
        assert_eq!(results.get(1).unwrap().rank, SearchRank::PartialMatch);
    }

    /// Boundary and sanity checks on query boundaries.
    #[test]
    fn test_query_boundaries() {
        let env = setup_test_env();

        // Empty query fails
        let empty_query = String::from_str(&env, "");
        assert!(InvoiceSearch::search_invoices(&env, empty_query).is_err());

        // Space-only query fails
        let space_query = String::from_str(&env, "     ");
        assert!(InvoiceSearch::search_invoices(&env, space_query).is_err());

        // Too long query (> 100 chars) fails
        let long_query = String::from_str(&env, &"a".repeat(101));
        assert!(InvoiceSearch::search_invoices(&env, long_query).is_err());
    }
}
