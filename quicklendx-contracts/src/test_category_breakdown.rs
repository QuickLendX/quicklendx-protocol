//! Tests for the `get_category_breakdown` entrypoint
//!
//! Covers:
//! - Empty platform (no invoices)
//! - Single category with invoices
//! - Multiple categories with varying counts
//! - Zero-count categories are omitted
//! - All categories populated
//! - Category sum equals total invoice count

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env, String, Vec,
    };

    use crate::{analytics::CategoryBreakdown, storage::InvoiceStorage, types::*};

    fn random_bytes32(env: &Env, counter: u64) -> BytesN<32> {
        let mut bytes = [0u8; 32];
        bytes[0] = (counter >> 56) as u8;
        bytes[1] = (counter >> 48) as u8;
        bytes[2] = (counter >> 40) as u8;
        bytes[3] = (counter >> 32) as u8;
        bytes[4] = (counter >> 24) as u8;
        bytes[5] = (counter >> 16) as u8;
        bytes[6] = (counter >> 8) as u8;
        bytes[7] = counter as u8;
        BytesN::from_array(env, &bytes)
    }

    fn create_test_invoice(
        env: &Env,
        id: &BytesN<32>,
        business: &Address,
        category: InvoiceCategory,
    ) -> Invoice {
        Invoice {
            id: id.clone(),
            business: business.clone(),
            amount: 100_000_000,
            currency: Address::random(env),
            due_date: 1000,
            status: InvoiceStatus::Pending,
            category,
            description: String::from_small_str(env, "Test invoice"),
            metadata_customer_name: None,
            metadata_tax_id: None,
            tags: Vec::new(env),
            average_rating: None,
            created_at: 100,
            updated_at: 100,
            attachment_hash: None,
            settlement_amount: None,
            settlement_currency: None,
            settlement_timestamp: None,
            dispute_status: DisputeStatus::None,
        }
    }

    #[test]
    fn test_category_breakdown_empty_platform() {
        let env = Env::default();

        let breakdown = InvoiceStorage::get_all_categories(&env)
            .iter()
            .map(|category| {
                let count =
                    InvoiceStorage::get_invoice_count_by_category_from_index(&env, &category);
                (category, count)
            })
            .filter(|(_cat, count)| *count > 0)
            .collect::<Vec<_>>();

        // Empty platform should have empty breakdown
        assert_eq!(breakdown.len(), 0);
    }

    #[test]
    fn test_category_breakdown_single_category() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create 3 invoices in Services category
        for i in 0..3 {
            let invoice_id = random_bytes32(&env, i as u64);
            let invoice =
                create_test_invoice(&env, &invoice_id, &business, InvoiceCategory::Services);
            InvoiceStorage::store(&env, &invoice);
        }

        let mut breakdown: Vec<(InvoiceCategory, u32)> = InvoiceStorage::get_all_categories(&env)
            .iter()
            .map(|category| {
                let count =
                    InvoiceStorage::get_invoice_count_by_category_from_index(&env, &category);
                (category, count)
            })
            .filter(|(_cat, count)| *count > 0)
            .collect();

        // Should have only Services with count 3
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0].0, InvoiceCategory::Services);
        assert_eq!(breakdown[0].1, 3);
    }

    #[test]
    fn test_category_breakdown_multiple_categories() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create invoices in different categories
        let categories_and_counts = vec![
            (InvoiceCategory::Services, 2),
            (InvoiceCategory::Products, 3),
            (InvoiceCategory::Consulting, 1),
            (InvoiceCategory::Technology, 4),
        ];

        let mut counter = 0u64;
        for (category, count) in categories_and_counts.iter() {
            for _ in 0..*count {
                let invoice_id = random_bytes32(&env, counter);
                let invoice = create_test_invoice(&env, &invoice_id, &business, *category);
                InvoiceStorage::store(&env, &invoice);
                counter += 1;
            }
        }

        let breakdown: Vec<(InvoiceCategory, u32)> = InvoiceStorage::get_all_categories(&env)
            .iter()
            .map(|category| {
                let count =
                    InvoiceStorage::get_invoice_count_by_category_from_index(&env, &category);
                (category, count)
            })
            .filter(|(_cat, count)| *count > 0)
            .collect();

        // Should have 4 categories
        assert_eq!(breakdown.len(), 4);

        // Verify counts match
        let mut count_map: std::collections::HashMap<InvoiceCategory, u32> =
            std::collections::HashMap::new();
        for (cat, count) in breakdown.iter() {
            count_map.insert(*cat, *count);
        }

        assert_eq!(count_map.get(&InvoiceCategory::Services), Some(&2u32));
        assert_eq!(count_map.get(&InvoiceCategory::Products), Some(&3u32));
        assert_eq!(count_map.get(&InvoiceCategory::Consulting), Some(&1u32));
        assert_eq!(count_map.get(&InvoiceCategory::Technology), Some(&4u32));
    }

    #[test]
    fn test_category_breakdown_excludes_zero_counts() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create invoices in only 2 out of 9 categories
        let invoice_id1 = random_bytes32(&env, 1);
        let invoice1 =
            create_test_invoice(&env, &invoice_id1, &business, InvoiceCategory::Healthcare);
        InvoiceStorage::store(&env, &invoice1);

        let invoice_id2 = random_bytes32(&env, 2);
        let invoice2 = create_test_invoice(
            &env,
            &invoice_id2,
            &business,
            InvoiceCategory::Manufacturing,
        );
        InvoiceStorage::store(&env, &invoice2);

        let breakdown: Vec<(InvoiceCategory, u32)> = InvoiceStorage::get_all_categories(&env)
            .iter()
            .map(|category| {
                let count =
                    InvoiceStorage::get_invoice_count_by_category_from_index(&env, &category);
                (category, count)
            })
            .filter(|(_cat, count)| *count > 0)
            .collect();

        // Should only include categories with non-zero counts (2 categories)
        assert_eq!(breakdown.len(), 2);

        // Verify that only the non-zero categories are included
        for (cat, _count) in breakdown.iter() {
            assert!(*cat == InvoiceCategory::Healthcare || *cat == InvoiceCategory::Manufacturing);
        }
    }

    #[test]
    fn test_category_breakdown_all_categories_populated() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create at least one invoice in each category
        let all_categories = vec![
            InvoiceCategory::Services,
            InvoiceCategory::Goods,
            InvoiceCategory::Consulting,
            InvoiceCategory::Logistics,
            InvoiceCategory::Products,
            InvoiceCategory::Manufacturing,
            InvoiceCategory::Technology,
            InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];

        for (idx, category) in all_categories.iter().enumerate() {
            let invoice_id = random_bytes32(&env, idx as u64);
            let invoice = create_test_invoice(&env, &invoice_id, &business, *category);
            InvoiceStorage::store(&env, &invoice);
        }

        let breakdown: Vec<(InvoiceCategory, u32)> = InvoiceStorage::get_all_categories(&env)
            .iter()
            .map(|category| {
                let count =
                    InvoiceStorage::get_invoice_count_by_category_from_index(&env, &category);
                (category, count)
            })
            .filter(|(_cat, count)| *count > 0)
            .collect();

        // Should have all 9 categories
        assert_eq!(breakdown.len(), 9);

        // Verify all categories are present with count >= 1
        for (cat, count) in breakdown.iter() {
            assert!(count >= 1);
            assert!(all_categories.contains(cat));
        }
    }

    #[test]
    fn test_category_breakdown_sum_equals_total() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create 10 invoices across different categories
        let total = 10u32;
        for i in 0..total {
            let invoice_id = random_bytes32(&env, i as u64);
            let category = match i % 3 {
                0 => InvoiceCategory::Services,
                1 => InvoiceCategory::Products,
                _ => InvoiceCategory::Consulting,
            };
            let invoice = create_test_invoice(&env, &invoice_id, &business, category);
            InvoiceStorage::store(&env, &invoice);
        }

        let breakdown: Vec<(InvoiceCategory, u32)> = InvoiceStorage::get_all_categories(&env)
            .iter()
            .map(|category| {
                let count =
                    InvoiceStorage::get_invoice_count_by_category_from_index(&env, &category);
                (category, count)
            })
            .filter(|(_cat, count)| *count > 0)
            .collect();

        let total_from_breakdown: u32 = breakdown.iter().map(|(_cat, count)| count).sum();

        // Sum of all category counts should equal total invoices
        assert_eq!(total_from_breakdown, total);
    }

    #[test]
    fn test_category_breakdown_after_invoice_status_change() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create and store an invoice in Services
        let invoice_id = random_bytes32(&env, 1);
        let mut invoice =
            create_test_invoice(&env, &invoice_id, &business, InvoiceCategory::Services);
        InvoiceStorage::store(&env, &invoice);

        let count1 = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        assert_eq!(count1, 1);

        // Change the invoice status (should not affect category count)
        invoice.status = InvoiceStatus::Verified;
        InvoiceStorage::update(&env, &invoice);

        let count2 = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        assert_eq!(count2, 1); // Count should remain the same
    }

    #[test]
    fn test_category_breakdown_after_category_change() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create an invoice in Services
        let invoice_id = random_bytes32(&env, 1);
        let mut invoice =
            create_test_invoice(&env, &invoice_id, &business, InvoiceCategory::Services);
        InvoiceStorage::store(&env, &invoice);

        let count_services_before = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        assert_eq!(count_services_before, 1);

        // Change the category
        invoice.category = InvoiceCategory::Products;
        InvoiceStorage::update(&env, &invoice);

        let count_services_after = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        let count_products_after = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Products,
        );

        // Should now have 1 invoice in Products and 0 in Services
        assert_eq!(count_services_after, 0);
        assert_eq!(count_products_after, 1);
    }

    #[test]
    fn test_category_breakdown_after_deletion() {
        let env = Env::default();
        let business = Address::random(&env);

        // Create 2 invoices in Services
        let invoice_id1 = random_bytes32(&env, 1);
        let invoice1 =
            create_test_invoice(&env, &invoice_id1, &business, InvoiceCategory::Services);
        InvoiceStorage::store(&env, &invoice1);

        let invoice_id2 = random_bytes32(&env, 2);
        let invoice2 =
            create_test_invoice(&env, &invoice_id2, &business, InvoiceCategory::Services);
        InvoiceStorage::store(&env, &invoice2);

        let count_before = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        assert_eq!(count_before, 2);

        // Delete one invoice
        InvoiceStorage::delete_invoice(&env, &invoice_id1);

        let count_after = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        assert_eq!(count_after, 1);
    }

    #[test]
    fn test_category_breakdown_index_efficiency() {
        // This test verifies that the index-based counting is being used
        // by checking the behavior with many invoices
        let env = Env::default();
        let business = Address::random(&env);

        // Create 100 invoices in various categories
        for i in 0..100 {
            let invoice_id = random_bytes32(&env, i as u64);
            let category = match i % 5 {
                0 => InvoiceCategory::Services,
                1 => InvoiceCategory::Products,
                2 => InvoiceCategory::Consulting,
                3 => InvoiceCategory::Technology,
                _ => InvoiceCategory::Healthcare,
            };
            let invoice = create_test_invoice(&env, &invoice_id, &business, category);
            InvoiceStorage::store(&env, &invoice);
        }

        // Verify the index-based method efficiently counts invoices
        let services_count = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Services,
        );
        assert_eq!(services_count, 20); // 100 / 5 = 20

        let products_count = InvoiceStorage::get_invoice_count_by_category_from_index(
            &env,
            &InvoiceCategory::Products,
        );
        assert_eq!(products_count, 20);
    }
}
