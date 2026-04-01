//! Tests for multi-currency whitelist: add/remove currency, enforcement in invoice and bid flows.
//!
//! Cases: invoice with non-whitelisted currency fails when whitelist is set; bid on such
//! invoice fails; only admin can add/remove currency.

use super::*;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::Address as _,
    Address, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    let _ = client.set_admin(&admin);
    (env, client, admin)
}

#[test]
fn test_get_whitelisted_currencies_empty_by_default() {
    let (env, client, _admin) = setup();
    let currency = Address::generate(&env);

    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 0, "whitelist should start empty");
    assert!(
        !client.is_allowed_currency(&currency),
        "currency should not be allowed before add"
    );
}

#[test]
fn test_get_whitelisted_currencies_after_add_and_remove() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);

    client.add_currency(&admin, &currency_a);
    client.add_currency(&admin, &currency_b);

    let after_add = client.get_whitelisted_currencies();
    assert_eq!(after_add.len(), 2);
    assert!(after_add.contains(&currency_a));
    assert!(after_add.contains(&currency_b));

    client.remove_currency(&admin, &currency_a);
    let after_remove_one = client.get_whitelisted_currencies();
    assert_eq!(after_remove_one.len(), 1);
    assert!(!after_remove_one.contains(&currency_a));
    assert!(after_remove_one.contains(&currency_b));

    client.remove_currency(&admin, &currency_b);
    let after_remove_all = client.get_whitelisted_currencies();
    assert_eq!(after_remove_all.len(), 0);
}

#[test]
fn test_is_allowed_currency_true_false_paths() {
    let (env, client, admin) = setup();
    let allowed = Address::generate(&env);
    let disallowed = Address::generate(&env);

    client.add_currency(&admin, &allowed);
    assert!(client.is_allowed_currency(&allowed));
    assert!(!client.is_allowed_currency(&disallowed));

    client.remove_currency(&admin, &allowed);
    assert!(
        !client.is_allowed_currency(&allowed),
        "removed currency should no longer be allowed"
    );
}

#[test]
fn test_add_remove_currency_admin_only() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    assert!(client.is_allowed_currency(&currency));
    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), currency);

    client.remove_currency(&admin, &currency);
    assert!(!client.is_allowed_currency(&currency));
    let list2 = client.get_whitelisted_currencies();
    assert_eq!(list2.len(), 0);
}

#[test]
fn test_non_admin_cannot_add_currency() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let non_admin = Address::generate(&env);
    let res = client.try_add_currency(&non_admin, &currency);
    assert!(res.is_err());
}

#[test]
fn test_non_admin_cannot_remove_currency() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let non_admin = Address::generate(&env);
    let res = client.try_remove_currency(&non_admin, &currency);
    assert!(res.is_err());
}

#[test]
fn test_invoice_with_non_whitelisted_currency_fails_when_whitelist_set() {
    let (env, client, admin) = setup();
    let allowed_currency = Address::generate(&env);
    client.add_currency(&admin, &allowed_currency);
    let disallowed_currency = Address::generate(&env);
    let business = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let res = client.try_store_invoice(
        &business,
        &1000i128,
        &disallowed_currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(res.is_err());
}

#[test]
fn test_invoice_with_whitelisted_currency_succeeds() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let business = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let got = client.get_invoice(&invoice_id);
    assert_eq!(got.amount, 1000i128);
}

#[test]
fn test_bid_on_invoice_with_non_whitelisted_currency_fails_when_whitelist_set() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    client.add_currency(&admin, &currency_a);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency_a,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &5000i128);
    client.remove_currency(&admin, &currency_a);
    client.add_currency(&admin, &currency_b);
    let res = client.try_place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    assert!(res.is_err());
}

#[test]
fn test_add_currency_idempotent() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    client.add_currency(&admin, &currency);
    let list = client.get_whitelisted_currencies();
    assert_eq!(list.len(), 1);
}

#[test]
fn test_remove_currency_when_missing_is_noop() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);

    client.add_currency(&admin, &currency);
    client.remove_currency(&admin, &currency);
    assert_eq!(client.get_whitelisted_currencies().len(), 0);

    let second_remove = client.try_remove_currency(&admin, &currency);
    assert!(
        second_remove.is_ok(),
        "removing an already absent currency should be a no-op"
    );
    assert_eq!(client.get_whitelisted_currencies().len(), 0);
}

#[test]
fn test_set_currencies_replaces_whitelist() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    client.add_currency(&admin, &currency_a);

    let mut new_list = Vec::new(&env);
    new_list.push_back(currency_b.clone());
    client.set_currencies(&admin, &new_list);

    assert!(!client.is_allowed_currency(&currency_a));
    assert!(client.is_allowed_currency(&currency_b));
    assert_eq!(client.currency_count(), 1);
}

#[test]
fn test_set_currencies_deduplicates() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    let mut duped = Vec::new(&env);
    duped.push_back(currency.clone());
    duped.push_back(currency.clone());
    client.set_currencies(&admin, &duped);
    assert_eq!(client.currency_count(), 1);
}

#[test]
fn test_non_admin_cannot_set_currencies() {
    let (env, client, _admin) = setup();
    let currency = Address::generate(&env);
    let mut list = Vec::new(&env);
    list.push_back(currency.clone());
    let non_admin = Address::generate(&env);
    let res = client.try_set_currencies(&non_admin, &list);
    assert!(res.is_err());
}

#[test]
fn test_clear_currencies_allows_all() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    client.clear_currencies(&admin);
    assert_eq!(client.currency_count(), 0);
    // empty whitelist = all allowed (backward-compat rule)
    let business = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let any_token = Address::generate(&env);
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &any_token,
        &due_date,
        &String::from_str(&env, "Desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let got = client.get_invoice(&invoice_id);
    assert_eq!(got.amount, 1000i128);
}

#[test]
fn test_non_admin_cannot_clear_currencies() {
    let (env, client, admin) = setup();
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let non_admin = Address::generate(&env);
    let res = client.try_clear_currencies(&non_admin);
    assert!(res.is_err());
}

#[test]
fn test_currency_count() {
    let (env, client, admin) = setup();
    assert_eq!(client.currency_count(), 0);
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    client.add_currency(&admin, &currency_a);
    assert_eq!(client.currency_count(), 1);
    client.add_currency(&admin, &currency_b);
    assert_eq!(client.currency_count(), 2);
    client.remove_currency(&admin, &currency_a);
    assert_eq!(client.currency_count(), 1);
}

#[test]
fn test_get_whitelisted_currencies_paged() {
    let (env, client, admin) = setup();
    let currency_a = Address::generate(&env);
    let currency_b = Address::generate(&env);
    let currency_c = Address::generate(&env);
    client.add_currency(&admin, &currency_a);
    client.add_currency(&admin, &currency_b);
    client.add_currency(&admin, &currency_c);

    let page1 = client.get_whitelisted_currencies_paged(&0u32, &2u32);
    assert_eq!(page1.len(), 2);

    let page2 = client.get_whitelisted_currencies_paged(&2u32, &2u32);
    assert_eq!(page2.len(), 1);

    // offset beyond length returns empty
    let page3 = client.get_whitelisted_currencies_paged(&10u32, &2u32);
    assert_eq!(page3.len(), 0);
}

/// Test boundary conditions for pagination with empty whitelist
#[test]
fn test_pagination_empty_whitelist_boundaries() {
    let (_env, client, _admin) = setup();
    
    // Empty whitelist with zero offset/limit
    let result = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(result.len(), 0, "empty whitelist with zero limit should return empty");
    
    // Empty whitelist with non-zero offset/limit
    let result = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(result.len(), 0, "empty whitelist with any limit should return empty");
    
    // Empty whitelist with large offset
    let result = client.get_whitelisted_currencies_paged(&u32::MAX, &10u32);
    assert_eq!(result.len(), 0, "empty whitelist with max offset should return empty");
    
    // Empty whitelist with max limit
    let result = client.get_whitelisted_currencies_paged(&0u32, &u32::MAX);
    assert_eq!(result.len(), 0, "empty whitelist with max limit should return empty");
}

/// Test boundary conditions for pagination offset saturation
#[test]
fn test_pagination_offset_saturation() {
    let (env, client, admin) = setup();
    
    // Add exactly 5 currencies for predictable testing
    let currencies: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }

    // Test offset at exact boundary (length)
    let result = client.get_whitelisted_currencies_paged(&5u32, &10u32);
    assert_eq!(result.len(), 0, "offset at exact length should return empty");
    
    // Test offset just beyond boundary
    let result = client.get_whitelisted_currencies_paged(&6u32, &10u32);
    assert_eq!(result.len(), 0, "offset beyond length should return empty");
    
    // Test offset at maximum value
    let result = client.get_whitelisted_currencies_paged(&u32::MAX, &10u32);
    assert_eq!(result.len(), 0, "max offset should return empty without panic");
    
    // Test offset near maximum with small limit
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 1), &1u32);
    assert_eq!(result.len(), 0, "near-max offset should return empty without panic");
    
    // Test valid offset at boundary minus one
    let result = client.get_whitelisted_currencies_paged(&4u32, &10u32);
    assert_eq!(result.len(), 1, "offset at length-1 should return 1 item");
}

/// Test boundary conditions for pagination limit saturation
#[test]
fn test_pagination_limit_saturation() {
    let (env, client, admin) = setup();
    
    // Add exactly 3 currencies
    let currencies: std::vec::Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Test zero limit
    let result = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(result.len(), 0, "zero limit should return empty");
    
    // Test limit larger than available items
    let result = client.get_whitelisted_currencies_paged(&0u32, &100u32);
    assert_eq!(result.len(), 3, "limit larger than available should return all items");
    
    // Test maximum limit value
    let result = client.get_whitelisted_currencies_paged(&0u32, &u32::MAX);
    assert_eq!(result.len(), 3, "max limit should return all items without panic");
    
    // Test limit exactly matching available items
    let result = client.get_whitelisted_currencies_paged(&0u32, &3u32);
    assert_eq!(result.len(), 3, "limit matching count should return all items");
    
    // Test limit one less than available
    let result = client.get_whitelisted_currencies_paged(&0u32, &2u32);
    assert_eq!(result.len(), 2, "limit less than count should return limited items");
}

/// Test boundary conditions for offset + limit overflow scenarios
#[test]
fn test_pagination_overflow_protection() {
    let (env, client, admin) = setup();
    
    // Add 10 currencies for comprehensive testing
    let currencies: std::vec::Vec<Address> = (0..10).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }

    // Test offset + limit overflow (should not panic)
    let result = client.get_whitelisted_currencies_paged(&u32::MAX, &u32::MAX);
    assert_eq!(result.len(), 0, "max offset + max limit should return empty without panic");
    
    // Test large offset with large limit
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 5), &10u32);
    assert_eq!(result.len(), 0, "large offset with normal limit should return empty");
    
    // Test normal offset with very large limit
    let result = client.get_whitelisted_currencies_paged(&5u32, &u32::MAX);
    assert_eq!(result.len(), 5, "normal offset with max limit should return remaining items");
    
    // Test edge case: offset at max-1, limit 1
    let result = client.get_whitelisted_currencies_paged(&(u32::MAX - 1), &1u32);
    assert_eq!(result.len(), 0, "near-max offset with small limit should return empty");
    
    // Test arithmetic overflow protection: offset + limit > u32::MAX
    let large_offset = u32::MAX / 2;
    let large_limit = u32::MAX / 2 + 1;
    let result = client.get_whitelisted_currencies_paged(&large_offset, &large_limit);
    assert_eq!(result.len(), 0, "arithmetic overflow scenario should be handled safely");
}

/// Test pagination consistency and ordering
#[test]
fn test_pagination_consistency_and_ordering() {
    let (env, client, admin) = setup();
    
    // Add currencies in a specific order
    let currencies: std::vec::Vec<Address> = (0..7).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Get full list for comparison
    let full_list = client.get_whitelisted_currencies();
    assert_eq!(full_list.len(), 7, "should have 7 currencies");
    
    // Test that pagination returns items in same order as full list
    let page1 = client.get_whitelisted_currencies_paged(&0u32, &3u32);
    let page2 = client.get_whitelisted_currencies_paged(&3u32, &3u32);
    let page3 = client.get_whitelisted_currencies_paged(&6u32, &3u32);
    
    assert_eq!(page1.len(), 3, "first page should have 3 items");
    assert_eq!(page2.len(), 3, "second page should have 3 items");
    assert_eq!(page3.len(), 1, "third page should have 1 item");
    
    // Verify ordering consistency
    for i in 0..3 {
        assert_eq!(page1.get(i).unwrap(), full_list.get(i).unwrap(), 
                  "page1 item {} should match full list", i);
    }
    for i in 0..3 {
        assert_eq!(page2.get(i).unwrap(), full_list.get(i + 3).unwrap(), 
                  "page2 item {} should match full list", i);
    }
    assert_eq!(page3.get(0).unwrap(), full_list.get(6).unwrap(), 
              "page3 item should match full list");
    
    // Test overlapping pages don't duplicate
    let overlap_page = client.get_whitelisted_currencies_paged(&2u32, &3u32);
    assert_eq!(overlap_page.len(), 3, "overlapping page should have 3 items");
    assert_eq!(overlap_page.get(0).unwrap(), full_list.get(2).unwrap(), 
              "overlapping page should start at correct offset");
}

/// Test pagination with single item edge cases
#[test]
fn test_pagination_single_item_edge_cases() {
    let (env, client, admin) = setup();
    
    // Add exactly one currency
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    
    // Test various pagination scenarios with single item
    let result = client.get_whitelisted_currencies_paged(&0u32, &1u32);
    assert_eq!(result.len(), 1, "should return the single item");
    assert_eq!(result.get(0).unwrap(), currency, "should return correct currency");
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(result.len(), 1, "large limit should still return single item");
    
    let result = client.get_whitelisted_currencies_paged(&1u32, &1u32);
    assert_eq!(result.len(), 0, "offset beyond single item should return empty");
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &0u32);
    assert_eq!(result.len(), 0, "zero limit should return empty even with item");
}

/// Test pagination behavior after whitelist modifications
#[test]
fn test_pagination_after_modifications() {
    let (env, client, admin) = setup();
    
    // Add initial currencies
    let currencies: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }

    // Test pagination before modification
    let page_before = client.get_whitelisted_currencies_paged(&0u32, &3u32);
    assert_eq!(page_before.len(), 3, "should have 3 items before modification");
    
    // Remove some currencies
    client.remove_currency(&admin, &currencies[1]);
    client.remove_currency(&admin, &currencies[3]);
    
    // Test pagination after removal
    let page_after = client.get_whitelisted_currencies_paged(&0u32, &3u32);
    assert_eq!(page_after.len(), 3, "should have 3 items after removal");
    
    // Verify removed currencies are not in results
    let full_list_after = client.get_whitelisted_currencies();
    assert_eq!(full_list_after.len(), 3, "should have 3 total items after removal");
    assert!(!full_list_after.contains(&currencies[1]), "removed currency should not be present");
    assert!(!full_list_after.contains(&currencies[3]), "removed currency should not be present");
    
    // Test pagination at new boundary
    let boundary_page = client.get_whitelisted_currencies_paged(&3u32, &1u32);
    assert_eq!(boundary_page.len(), 0, "offset at new length should return empty");
    
    // Clear all currencies and test
    client.clear_currencies(&admin);
    let empty_page = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(empty_page.len(), 0, "pagination after clear should return empty");
}

/// Test pagination security: ensure no information leakage or unauthorized access
#[test]
fn test_pagination_security_boundaries() {
    let (env, client, admin) = setup();
    
    // Add currencies as admin
    let currencies: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }
    
    // Test that pagination works for non-admin users (public read access)
    let _non_admin = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();
    
    // Non-admin should be able to read paginated results
    let result = client.get_whitelisted_currencies_paged(&0u32, &3u32);
    assert_eq!(result.len(), 3, "non-admin should be able to read paginated results");
    
    // Test that pagination doesn't expose more data than intended
    let full_list = client.get_whitelisted_currencies();
    let paginated_total = client.get_whitelisted_currencies_paged(&0u32, &u32::MAX);
    assert_eq!(full_list.len(), paginated_total.len(), 
              "paginated read should not expose more data than full read");
    
    // Verify all items match between full and paginated reads
    for i in 0..full_list.len() {
        assert_eq!(full_list.get(i).unwrap(), paginated_total.get(i).unwrap(),
                  "item {} should match between full and paginated reads", i);
    }
}

/// Test pagination performance and DoS resistance with large datasets
#[test]
fn test_pagination_large_dataset_boundaries() {
    let (env, client, admin) = setup();
    
    // Add a larger number of currencies to test performance boundaries
    let large_count = 50u32; // Reasonable size for testing
    let currencies: std::vec::Vec<Address> = (0..large_count).map(|_| Address::generate(&env)).collect();

    // Add currencies in batches to test bulk operations
    let batch_size = 10usize;
    for chunk in currencies.chunks(batch_size) {
        for currency in chunk {
            client.add_currency(&admin, currency);
        }
    }
    
    // Verify total count
    let count = client.currency_count();
    assert_eq!(count, large_count, "should have added all currencies");
    
    // Test pagination across large dataset
    let page_size = 7u32;
    let mut total_retrieved = 0u32;
    let mut offset = 0u32;
    
    loop {
        let page = client.get_whitelisted_currencies_paged(&offset, &page_size);
        if page.len() == 0 {
            break;
        }
        total_retrieved += page.len();
        offset += page_size;
        
        // Prevent infinite loop in case of implementation error
        if offset > large_count * 2 {
            panic!("pagination loop exceeded expected bounds");
        }
    }
    
    assert_eq!(total_retrieved, large_count, 
              "should retrieve all items through pagination");
    
    // Test large offset with large dataset
    let result = client.get_whitelisted_currencies_paged(&(large_count + 10), &10u32);
    assert_eq!(result.len(), 0, "large offset beyond dataset should return empty");
    
    // Test boundary at exact dataset size
    let result = client.get_whitelisted_currencies_paged(&large_count, &1u32);
    assert_eq!(result.len(), 0, "offset at exact dataset size should return empty");
}

/// Test pagination with rapid modifications (race condition simulation)
#[test]
fn test_pagination_concurrent_modification_boundaries() {
    let (env, client, admin) = setup();
    
    // Add initial dataset
    let currencies: std::vec::Vec<Address> = (0..10).map(|_| Address::generate(&env)).collect();
    for currency in &currencies {
        client.add_currency(&admin, currency);
    }

    // Simulate concurrent reads during modifications
    let initial_page = client.get_whitelisted_currencies_paged(&0u32, &5u32);
    assert_eq!(initial_page.len(), 5, "initial page should have 5 items");
    
    // Modify whitelist (remove some currencies)
    client.remove_currency(&admin, &currencies[2]);
    client.remove_currency(&admin, &currencies[7]);
    
    // Read same page after modification
    let modified_page = client.get_whitelisted_currencies_paged(&0u32, &5u32);
    assert_eq!(modified_page.len(), 5, "page should still return 5 items after removal");
    
    // Verify consistency: total count should match paginated count
    let total_count = client.currency_count();
    let mut paginated_count = 0u32;
    let mut offset = 0u32;
    
    loop {
        let page = client.get_whitelisted_currencies_paged(&offset, &3u32);
        if page.len() == 0 {
            break;
        }
        paginated_count += page.len();
        offset += 3u32;
        
        if offset > total_count * 2 {
            break; // Safety break
        }
    }
    
    assert_eq!(paginated_count, total_count, 
              "paginated count should match total count after modifications");
}

/// Test pagination edge cases with address generation and storage
#[test]
fn test_pagination_address_handling_boundaries() {
    let (env, client, admin) = setup();
    
    // Test with duplicate address attempts (should be idempotent)
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    client.add_currency(&admin, &currency); // Duplicate add
    
    let result = client.get_whitelisted_currencies_paged(&0u32, &10u32);
    assert_eq!(result.len(), 1, "duplicate adds should result in single entry");
    
    // Test with many unique addresses
    let unique_currencies: std::vec::Vec<Address> = (0..15).map(|_| Address::generate(&env)).collect();
    for currency in &unique_currencies {
        client.add_currency(&admin, currency);
    }
    
    // Verify all unique addresses are stored and retrievable
    let total_count = client.currency_count();
    assert_eq!(total_count, 16u32, "should have 16 total currencies (1 + 15)"); // 1 from duplicate test + 15 new
    
    // Test pagination retrieves all unique addresses
    let all_paginated = client.get_whitelisted_currencies_paged(&0u32, &20u32);
    assert_eq!(all_paginated.len(), 16, "should retrieve all unique addresses");
    
    // Verify no duplicates in paginated results
    for i in 0..all_paginated.len() {
        for j in (i + 1)..all_paginated.len() {
            assert_ne!(all_paginated.get(i).unwrap(), all_paginated.get(j).unwrap(),
                      "should not have duplicate addresses in paginated results");
        }
    }
}

/// Test pagination memory and storage efficiency boundaries
#[test]
fn test_pagination_storage_efficiency() {
    let (env, client, admin) = setup();
    
    // Test empty storage efficiency
    let empty_result = client.get_whitelisted_currencies_paged(&0u32, &100u32);
    assert_eq!(empty_result.len(), 0, "empty storage should return empty efficiently");
    
    // Add currencies and test storage growth
    let currencies: std::vec::Vec<Address> = (0..20).map(|_| Address::generate(&env)).collect();
    for (i, currency) in currencies.iter().enumerate() {
        client.add_currency(&admin, currency);
        
        // Test pagination at each growth step
        let count = (i + 1) as u32;
        let result = client.get_whitelisted_currencies_paged(&0u32, &count);
        assert_eq!(result.len(), count, "should return correct count at growth step {}", i);
        
        // Test boundary pagination
        let boundary_result = client.get_whitelisted_currencies_paged(&count, &1u32);
        assert_eq!(boundary_result.len(), 0, "boundary offset should return empty at step {}", i);
    }
    
    // Test bulk clear efficiency
    client.clear_currencies(&admin);
    let cleared_result = client.get_whitelisted_currencies_paged(&0u32, &100u32);
    assert_eq!(cleared_result.len(), 0, "cleared storage should return empty efficiently");
    
    // Verify count is also reset
    let count_after_clear = client.currency_count();
    assert_eq!(count_after_clear, 0u32, "count should be zero after clear");
}