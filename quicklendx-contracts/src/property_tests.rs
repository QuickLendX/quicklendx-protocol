#![cfg(test)]

use super::*;
use proptest::prelude::*;
use quickcheck::{quickcheck, TestResult};
use quickcheck_macros::quickcheck;
use soroban_sdk::{
    testutils::Address as _,
    vec, Address, BytesN, Env, String, Vec,
};
use crate::{
    invoice::{InvoiceCategory, InvoiceStatus, InvoiceStorage},
    bid::{BidStatus, BidStorage},
    payments::{EscrowStatus, EscrowStorage},
    profits::calculate_profit,
};

/// Property test setup helper
struct PropertyTestSetup<'a> {
    env: Env,
    client: QuickLendXContractClient<'a>,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
}

impl PropertyTestSetup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, QuickLendXContract);
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let currency = Address::generate(&env);
        
        // Set up admin and verify business
        client.set_admin(&admin);
        let kyc_data = String::from_str(&env, "Business registration documents");
        client.submit_kyc_application(&business, &kyc_data).unwrap();
        client.verify_business(&admin, &business).unwrap();
        
        Self {
            env,
            client,
            admin,
            business,
            investor,
            currency,
        }
    }
}

/// Property: Invoice amounts should always be positive
#[quickcheck]
fn prop_invoice_amounts_positive(amount: i128) -> TestResult {
    if amount <= 0 {
        return TestResult::discard();
    }
    
    let setup = PropertyTestSetup::new();
    let due_date = setup.env.ledger().timestamp() + 86400;
    let description = String::from_str(&setup.env, "Property test invoice");
    let category = InvoiceCategory::Services;
    let tags = vec![&setup.env, String::from_str(&setup.env, "property-test")];
    
    let result = setup.client.try_upload_invoice(
        &setup.business,
        &amount,
        &setup.currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    
    if let Ok(invoice_id) = result {
        let invoice = setup.client.get_invoice(&invoice_id);
        TestResult::from_bool(invoice.amount == amount && invoice.amount > 0)
    } else {
        TestResult::failed()
    }
}

/// Property: Due dates should always be in the future
#[quickcheck]
fn prop_due_dates_future(days_offset: u32) -> TestResult {
    if days_offset == 0 {
        return TestResult::discard();
    }
    
    let setup = PropertyTestSetup::new();
    let current_time = setup.env.ledger().timestamp();
    let due_date = current_time + (days_offset as u64 * 86400);
    let description = String::from_str(&setup.env, "Future due date test");
    let category = InvoiceCategory::Services;
    let tags = vec![&setup.env, String::from_str(&setup.env, "future-test")];
    
    let result = setup.client.try_upload_invoice(
        &setup.business,
        &1000,
        &setup.currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    
    if let Ok(invoice_id) = result {
        let invoice = setup.client.get_invoice(&invoice_id);
        TestResult::from_bool(invoice.due_date > current_time)
    } else {
        TestResult::failed()
    }
}

/// Property: Bid amounts should not exceed invoice amounts significantly
#[quickcheck]
fn prop_bid_amounts_reasonable(invoice_amount: u32, bid_percentage: u8) -> TestResult {
    if invoice_amount == 0 || bid_percentage == 0 || bid_percentage > 200 {
        return TestResult::discard();
    }
    
    let setup = PropertyTestSetup::new();
    let invoice_amount = invoice_amount as i128;
    let bid_amount = (invoice_amount * bid_percentage as i128) / 100;
    
    if bid_amount <= 0 {
        return TestResult::discard();
    }
    
    // Create invoice
    let due_date = setup.env.ledger().timestamp() + 86400;
    let description = String::from_str(&setup.env, "Bid test invoice");
    let category = InvoiceCategory::Services;
    let tags = vec![&setup.env, String::from_str(&setup.env, "bid-test")];
    
    let invoice_id = setup.client.upload_invoice(
        &setup.business,
        &invoice_amount,
        &setup.currency,
        &due_date,
        &description,
        &category,
        &tags,
    ).unwrap();
    
    setup.client.verify_invoice(&invoice_id).unwrap();
    
    // Place bid
    let expected_return = bid_amount + (bid_amount / 10); // 10% return
    let bid_id = setup.client.place_bid(
        &setup.investor,
        &invoice_id,
        &bid_amount,
        &expected_return,
    );
    
    let bid = setup.client.get_bid(&bid_id).unwrap();
    TestResult::from_bool(
        bid.bid_amount == bid_amount && 
        bid.expected_return >= bid_amount &&
        bid.status == BidStatus::Placed
    )
}

/// Property: Profit calculations should be consistent
#[quickcheck]
fn prop_profit_calculations_consistent(
    investment: u32,
    payment: u32,
    fee_bps: u16,
) -> TestResult {
    if investment == 0 || payment == 0 || fee_bps > 10000 {
        return TestResult::discard();
    }
    
    let investment_amount = investment as i128;
    let payment_amount = payment as i128;
    let platform_fee_bps = fee_bps as i128;
    
    let (profit, platform_fee) = calculate_profit(
        investment_amount,
        payment_amount,
        platform_fee_bps,
    );
    
    // Properties to verify:
    // 1. Platform fee should be calculated correctly
    let expected_platform_fee = (payment_amount * platform_fee_bps) / 10000;
    
    // 2. Profit should be payment minus investment minus platform fee
    let expected_profit = payment_amount - investment_amount - expected_platform_fee;
    
    TestResult::from_bool(
        platform_fee == expected_platform_fee &&
        profit == expected_profit
    )
}

/// Property: Escrow operations should maintain balance
#[quickcheck]
fn prop_escrow_balance_maintained(amount: u32) -> TestResult {
    if amount == 0 {
        return TestResult::discard();
    }
    
    let setup = PropertyTestSetup::new();
    let escrow_amount = amount as i128;
    
    // Create and fund invoice
    let due_date = setup.env.ledger().timestamp() + 86400;
    let description = String::from_str(&setup.env, "Escrow test invoice");
    let category = InvoiceCategory::Services;
    let tags = vec![&setup.env, String::from_str(&setup.env, "escrow-test")];
    
    let invoice_id = setup.client.upload_invoice(
        &setup.business,
        &escrow_amount,
        &setup.currency,
        &due_date,
        &description,
        &category,
        &tags,
    ).unwrap();
    
    setup.client.verify_invoice(&invoice_id).unwrap();
    
    // Place and accept bid
    let bid_id = setup.client.place_bid(
        &setup.investor,
        &invoice_id,
        &escrow_amount,
        &(escrow_amount + 100),
    );
    
    setup.client.accept_bid(&invoice_id, &bid_id);
    
    // Verify escrow details
    let escrow_details = setup.client.get_escrow_details(&invoice_id);
    let escrow_status = setup.client.get_escrow_status(&invoice_id);
    
    TestResult::from_bool(
        escrow_details.amount == escrow_amount &&
        escrow_status == EscrowStatus::Held &&
        escrow_details.investor == setup.investor &&
        escrow_details.business == setup.business
    )
}

/// Property: Invoice status transitions should be valid
proptest! {
    #[test]
    fn prop_invoice_status_transitions(
        initial_status in prop::sample::select(vec![
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
        ])
    ) {
        let setup = PropertyTestSetup::new();
        
        // Create invoice
        let due_date = setup.env.ledger().timestamp() + 86400;
        let description = String::from_str(&setup.env, "Status transition test");
        let category = InvoiceCategory::Services;
        let tags = vec![&setup.env, String::from_str(&setup.env, "status-test")];
        
        let invoice_id = setup.client.upload_invoice(
            &setup.business,
            &1000,
            &setup.currency,
            &due_date,
            &description,
            &category,
            &tags,
        ).unwrap();
        
        // Verify initial status is Pending
        let invoice = setup.client.get_invoice(&invoice_id);
        prop_assert_eq!(invoice.status, InvoiceStatus::Pending);
        
        // Test valid transitions
        match initial_status {
            InvoiceStatus::Pending => {
                // Can transition to Verified
                setup.client.verify_invoice(&invoice_id).unwrap();
                let invoice = setup.client.get_invoice(&invoice_id);
                prop_assert_eq!(invoice.status, InvoiceStatus::Verified);
            }
            InvoiceStatus::Verified => {
                // First verify, then can transition to Funded
                setup.client.verify_invoice(&invoice_id).unwrap();
                
                let bid_id = setup.client.place_bid(
                    &setup.investor,
                    &invoice_id,
                    &950,
                    &1050,
                );
                setup.client.accept_bid(&invoice_id, &bid_id);
                
                let invoice = setup.client.get_invoice(&invoice_id);
                prop_assert_eq!(invoice.status, InvoiceStatus::Funded);
            }
            InvoiceStatus::Funded => {
                // First verify and fund
                setup.client.verify_invoice(&invoice_id).unwrap();
                
                let bid_id = setup.client.place_bid(
                    &setup.investor,
                    &invoice_id,
                    &950,
                    &1050,
                );
                setup.client.accept_bid(&invoice_id, &bid_id);
                
                // Then can settle
                setup.client.settle_invoice(
                    &invoice_id,
                    &1000,
                    &setup.admin,
                    &250,
                ).unwrap();
                
                let invoice = setup.client.get_invoice(&invoice_id);
                prop_assert_eq!(invoice.status, InvoiceStatus::Paid);
            }
        }
    }
}

/// Property: Bid competition should select best offer
proptest! {
    #[test]
    fn prop_bid_competition_selects_best(
        bid_amounts in prop::collection::vec(1000u32..2000u32, 2..10)
    ) {
        let setup = PropertyTestSetup::new();
        
        // Create invoice
        let due_date = setup.env.ledger().timestamp() + 86400;
        let description = String::from_str(&setup.env, "Competition test");
        let category = InvoiceCategory::Services;
        let tags = vec![&setup.env, String::from_str(&setup.env, "competition")];
        
        let invoice_id = setup.client.upload_invoice(
            &setup.business,
            &2000,
            &setup.currency,
            &due_date,
            &description,
            &category,
            &tags,
        ).unwrap();
        
        setup.client.verify_invoice(&invoice_id).unwrap();
        
        // Place multiple bids
        let mut bid_ids = Vec::new();
        let mut max_amount = 0i128;
        let mut best_bid_id = None;
        
        for (i, &amount) in bid_amounts.iter().enumerate() {
            let investor = Address::generate(&setup.env);
            let bid_amount = amount as i128;
            let expected_return = bid_amount + 100;
            
            let bid_id = setup.client.place_bid(
                &investor,
                &invoice_id,
                &bid_amount,
                &expected_return,
            );
            
            bid_ids.push(bid_id.clone());
            
            if bid_amount > max_amount {
                max_amount = bid_amount;
                best_bid_id = Some(bid_id);
            }
        }
        
        // Accept the best bid
        if let Some(best_bid) = best_bid_id {
            setup.client.accept_bid(&invoice_id, &best_bid);
            
            // Verify the best bid was accepted
            let accepted_bid = setup.client.get_bid(&best_bid).unwrap();
            prop_assert_eq!(accepted_bid.status, BidStatus::Accepted);
            prop_assert_eq!(accepted_bid.bid_amount, max_amount);
            
            // Verify invoice is funded with the best amount
            let invoice = setup.client.get_invoice(&invoice_id);
            prop_assert_eq!(invoice.status, InvoiceStatus::Funded);
            prop_assert_eq!(invoice.funded_amount, max_amount);
        }
    }
}

/// Property: Audit trail should be immutable and complete
proptest! {
    #[test]
    fn prop_audit_trail_immutable(
        operations in prop::collection::vec(0u8..4u8, 1..10)
    ) {
        let setup = PropertyTestSetup::new();
        
        // Create invoice
        let due_date = setup.env.ledger().timestamp() + 86400;
        let description = String::from_str(&setup.env, "Audit trail test");
        let category = InvoiceCategory::Services;
        let tags = vec![&setup.env, String::from_str(&setup.env, "audit")];
        
        let invoice_id = setup.client.upload_invoice(
            &setup.business,
            &1000,
            &setup.currency,
            &due_date,
            &description,
            &category,
            &tags,
        ).unwrap();
        
        let mut expected_operations = 1; // Invoice creation
        
        // Perform various operations
        for &op in &operations {
            match op {
                0 => {
                    // Verify invoice
                    if setup.client.try_verify_invoice(&invoice_id).is_ok() {
                        expected_operations += 1;
                    }
                }
                1 => {
                    // Place bid (if invoice is verified)
                    let invoice = setup.client.get_invoice(&invoice_id);
                    if invoice.status == InvoiceStatus::Verified {
                        let investor = Address::generate(&setup.env);
                        setup.client.place_bid(&investor, &invoice_id, &950, &1050);
                        expected_operations += 1;
                    }
                }
                2 => {
                    // Accept bid (if bids exist)
                    let bids = setup.client.get_bids_for_invoice(&invoice_id);
                    if !bids.is_empty() {
                        let bid_id = bids.get(0).unwrap();
                        if setup.client.try_accept_bid(&invoice_id, &bid_id).is_ok() {
                            expected_operations += 1;
                        }
                    }
                }
                3 => {
                    // Release escrow (if funded)
                    let invoice = setup.client.get_invoice(&invoice_id);
                    if invoice.status == InvoiceStatus::Funded {
                        if setup.client.try_release_escrow_funds(&invoice_id).is_ok() {
                            expected_operations += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        
        // Verify audit trail completeness
        let audit_trail = setup.client.get_invoice_audit_trail(&invoice_id);
        prop_assert!(!audit_trail.is_empty());
        
        // Verify audit integrity
        let is_valid = setup.client.validate_invoice_audit_integrity(&invoice_id);
        prop_assert!(is_valid);
    }
}
