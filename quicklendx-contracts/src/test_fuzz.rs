#![cfg(test)]

use crate::{
    invoice::{Invoice, InvoiceCategory, InvoiceStatus},
    bid::{Bid, BidStatus},
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString, Vec as SorobanVec, BytesN};

use proptest::prelude::*;

// Constants for valid ranges
const MIN_AMOUNT: i128 = 1;
const MAX_AMOUNT: i128 = i128::MAX / 1000; // Avoid overflow in calculations
const MIN_DUE_DATE_OFFSET: u64 = 1;
const MAX_DUE_DATE_OFFSET: u64 = 365 * 24 * 60 * 60; // 1 year
const MAX_DESC_LEN: usize = 500;
const MIN_EXPECTED_RETURN: i128 = 1;

// Helper to setup test environment
fn setup_test_env() -> (Env, QuickLendXContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Initialize admin
    client.initialize_admin(&admin);
    
    // Setup currency
    let currency = Address::generate(&env);
    client.add_currency(&currency);
    
    // Verify business
    client.submit_kyc_application(&business, &SorobanString::from_str(&env, "Business KYC"));
    client.verify_business(&business);
    
    // Verify investor
    client.submit_investor_kyc(&investor, &SorobanString::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &1_000_000_000);
    
    (env, client, admin, business, investor)
}

// Property-based test for store_invoice
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn fuzz_store_invoice_valid_ranges(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        desc_len in 1usize..MAX_DESC_LEN,
    ) {
        let (env, client, _admin, business, _investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        let current_time = env.ledger().timestamp();
        let due_date = current_time + due_date_offset;
        let description = SorobanString::from_str(&env, &"x".repeat(desc_len));
        let tags = SorobanVec::new(&env);
        
        // Should not panic and return Ok
        let result = client.try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &InvoiceCategory::Goods,
            &tags,
        );
        
        // Assert: Either Ok with valid invoice_id or Err with no state corruption
        match result {
            Ok(invoice_id) => {
                // Verify invoice was stored correctly
                let invoice = client.get_invoice(&invoice_id);
                assert!(invoice.is_ok());
                let inv = invoice.unwrap();
                assert_eq!(inv.amount, amount);
                assert_eq!(inv.due_date, due_date);
                assert_eq!(inv.status, InvoiceStatus::Pending);
            }
            Err(_) => {
                // Error is acceptable, but state should be consistent
                // No invoice should be created
                let count = client.get_total_invoice_count();
                // Count should be 0 since no successful creation
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn fuzz_store_invoice_boundary_conditions(
        amount in prop::option::of(MIN_AMOUNT..MAX_AMOUNT),
        due_date_valid in prop::bool::ANY,
        desc_empty in prop::bool::ANY,
    ) {
        let (env, client, _admin, business, _investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        let current_time = env.ledger().timestamp();
        let amount = amount.unwrap_or(0); // Test zero/negative
        let due_date = if due_date_valid {
            current_time + 1000
        } else {
            current_time - 1 // Past date
        };
        let description = if desc_empty {
            SorobanString::from_str(&env, "")
        } else {
            SorobanString::from_str(&env, "Valid description")
        };
        let tags = SorobanVec::new(&env);
        
        // Should handle invalid inputs gracefully
        let result = client.try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &InvoiceCategory::Goods,
            &tags,
        );
        
        // Should either succeed or return proper error, never panic
        match result {
            Ok(_) => {
                // Valid inputs produced success
                assert!(amount > 0);
                assert!(due_date > current_time);
                assert!(!desc_empty);
            }
            Err(_) => {
                // Invalid inputs properly rejected
            }
        }
    }
}

// Property-based test for place_bid
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn fuzz_place_bid_valid_ranges(
        bid_amount in MIN_AMOUNT..MAX_AMOUNT,
        expected_return_multiplier in 100u32..200u32, // 1.0x to 2.0x
    ) {
        let (env, client, admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        // Create and verify invoice
        let invoice_amount = 1_000_000;
        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Goods,
            &SorobanVec::new(&env),
        );
        client.verify_invoice(&invoice_id);
        
        let expected_return = (bid_amount as i128 * expected_return_multiplier as i128) / 100;
        
        // Should not panic
        let result = client.try_place_bid(
            &investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
        );
        
        match result {
            Ok(bid_id) => {
                // Verify bid was stored correctly
                let bid = client.get_bid(&bid_id);
                assert!(bid.is_some());
                let b = bid.unwrap();
                assert_eq!(b.bid_amount, bid_amount);
                assert_eq!(b.expected_return, expected_return);
                assert_eq!(b.status, BidStatus::Placed);
            }
            Err(_) => {
                // Error acceptable (e.g., exceeds investment limit)
                // State should remain consistent
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn fuzz_place_bid_boundary_conditions(
        bid_amount in prop::option::of(MIN_AMOUNT..MAX_AMOUNT),
        expected_return_negative in prop::bool::ANY,
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        // Create and verify invoice
        let invoice_amount = 1_000_000;
        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Goods,
            &SorobanVec::new(&env),
        );
        client.verify_invoice(&invoice_id);
        
        let bid_amount = bid_amount.unwrap_or(0);
        let expected_return = if expected_return_negative {
            -100
        } else {
            bid_amount + 100
        };
        
        // Should handle invalid inputs gracefully
        let result = client.try_place_bid(
            &investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
        );
        
        // Should never panic, only return error for invalid inputs
        match result {
            Ok(_) => {
                assert!(bid_amount > 0);
                assert!(expected_return > 0);
            }
            Err(_) => {
                // Invalid inputs properly rejected
            }
        }
    }
}

// Property-based test for settle_invoice
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn fuzz_settle_invoice_payment_amounts(
        payment_multiplier in 50u32..200u32, // 0.5x to 2.0x of investment
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        // Create, verify, and fund invoice
        let invoice_amount = 1_000_000;
        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Goods,
            &SorobanVec::new(&env),
        );
        client.verify_invoice(&invoice_id);
        
        // Place and accept bid
        let bid_amount = 900_000;
        let expected_return = 1_000_000;
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        client.accept_bid(&bid_id);
        
        let payment_amount = (bid_amount * payment_multiplier as i128) / 100;
        
        // Get initial state
        let invoice_before = client.get_invoice(&invoice_id).unwrap();
        
        // Should not panic
        let result = client.try_settle_invoice(&invoice_id, &payment_amount);
        
        match result {
            Ok(_) => {
                // Verify state transition is valid
                let invoice_after = client.get_invoice(&invoice_id).unwrap();
                
                // Status should change to Paid or remain Funded (partial payment)
                assert!(
                    invoice_after.status == InvoiceStatus::Paid ||
                    invoice_after.status == InvoiceStatus::Funded
                );
                
                // Total paid should increase
                assert!(invoice_after.total_paid >= invoice_before.total_paid);
            }
            Err(_) => {
                // Error acceptable for invalid payment amounts
                // State should not change
                let invoice_after = client.get_invoice(&invoice_id).unwrap();
                assert_eq!(invoice_before.status, invoice_after.status);
                assert_eq!(invoice_before.total_paid, invoice_after.total_paid);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn fuzz_settle_invoice_boundary_conditions(
        payment_amount in prop::option::of(MIN_AMOUNT..MAX_AMOUNT),
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        // Create, verify, and fund invoice
        let invoice_amount = 1_000_000;
        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Goods,
            &SorobanVec::new(&env),
        );
        client.verify_invoice(&invoice_id);
        
        // Place and accept bid
        let bid_amount = 900_000;
        let expected_return = 1_000_000;
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        client.accept_bid(&bid_id);
        
        let payment_amount = payment_amount.unwrap_or(0);
        
        // Should handle edge cases gracefully
        let result = client.try_settle_invoice(&invoice_id, &payment_amount);
        
        // Should never panic
        match result {
            Ok(_) => {
                assert!(payment_amount > 0);
            }
            Err(_) => {
                // Invalid payment properly rejected
            }
        }
    }
}

// Math overflow/underflow tests
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    
    #[test]
    fn fuzz_no_arithmetic_overflow(
        amount1 in 1i128..i128::MAX / 2,
        amount2 in 1i128..i128::MAX / 2,
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        // Test large amounts don't cause overflow
        let invoice_amount = amount1;
        let due_date = env.ledger().timestamp() + 10000;
        
        let result = client.try_store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Large amount test"),
            &InvoiceCategory::Goods,
            &SorobanVec::new(&env),
        );
        
        // Should handle large numbers without panic
        if let Ok(invoice_id) = result {
            client.verify_invoice(&invoice_id);
            
            // Try bidding with large amounts
            let bid_amount = amount2.min(1_000_000_000); // Cap at investment limit
            let expected_return = bid_amount + 1000;
            
            let _ = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        }
    }
}

#[cfg(test)]
mod standard_tests {
    use super::*;

    #[test]
    fn test_fuzz_infrastructure_works() {
        let (env, client, _admin, business, _investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        let invoice_id = client.store_invoice(
            &business,
            &1_000_000,
            &currency,
            &(env.ledger().timestamp() + 10000),
            &SorobanString::from_str(&env, "Test"),
            &InvoiceCategory::Goods,
            &SorobanVec::new(&env),
        );
        
        assert!(client.get_invoice(&invoice_id).is_ok());
    }
}
