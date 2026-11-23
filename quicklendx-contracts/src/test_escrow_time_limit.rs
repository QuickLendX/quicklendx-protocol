//! Tests for the escrow-time-limit guard.
//!
//! # Boundary locked in by this module
//!
//! The contract's bidding path (via [`verification::validate_bid_placement`])
//! enforces a temporal rule: bidding is only permitted before the invoice's
//! `due_date`. This prevents investors from placing bids on invoices that have
//! already matured.
//!
//! The guard is implemented as:
//! ```rust
//! if env.ledger().timestamp() >= invoice.due_date {
//!     return Err(QuickLendXError::InvalidStatus);
//! }
//! ```
//!
//! | Test | Description |
//! |---|---|
//! | `bid_succeeds_within_time_limit` | Happy path: bid placed when timestamp < due_date |
//! | `bid_blocked_at_time_limit` | Sad path: bid blocked when timestamp == due_date (>= boundary) |
//! | `bid_blocked_past_time_limit` | Sad path: bid blocked when timestamp > due_date |

#![cfg(test)]

use crate::{
    errors::QuickLendXError,
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    
    client.set_protocol_limits(
        &admin,
        100, // min_invoice_amount
        10,  // min_bid_amount
        10,  // min_bid_bps
        365, // max_due_date_days
        7 * 24 * 60 * 60, // grace_period_seconds
        0,   // max_invoices_per_business
        crate::verification::InvestorTier::None,
    );
    let business = Address::generate(env);
    client.submit_kyc_application(
        &admin,
        &business,
        &String::from_str(env, "business"),
        &String::from_str(env, "tax1"),
        &String::from_str(env, "address"),
        &String::from_str(env, "data"),
    );
    client.approve_kyc(&admin, &business);
    (client, admin, business)
}

fn create_test_invoice(env: &Env, client: &QuickLendXContractClient<'static>, business: &Address, admin: &Address, due_date: u64) -> soroban_sdk::BytesN<32> {
    let currency = Address::generate(env);
    let invoice_id = client.store_invoice(
        business,
        &100,
        &currency,
        &due_date,
        &String::from_str(env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice_data(admin, &invoice_id);
    invoice_id
}

#[test]
fn bid_succeeds_within_time_limit() {
    let env = Env::default();
    env.mock_all_auths();
    
    let now = 100_000_000u64;
    env.ledger().set_timestamp(now);
    
    let (client, admin, business) = setup(&env);
    
    // due_date is in the future
    let due_date = now + 10_000;
    let invoice_id = create_test_invoice(&env, &client, &business, &admin, due_date);
    
    // Advance time to BEFORE the due_date
    env.ledger().set_timestamp(due_date - 1);
    
    let investor = Address::generate(&env);
    let result = client.try_place_bid(
        &invoice_id,
        &investor,
        &50,
        &55,
    );
    assert!(result.is_ok());
}

#[test]
fn bid_blocked_at_time_limit() {
    let env = Env::default();
    env.mock_all_auths();
    
    let now = 100_000_000u64;
    env.ledger().set_timestamp(now);
    
    let (client, admin, business) = setup(&env);
    
    // due_date is in the future
    let due_date = now + 10_000;
    let invoice_id = create_test_invoice(&env, &client, &business, &admin, due_date);
    
    // Advance time to EXACTLY the due_date
    env.ledger().set_timestamp(due_date);
    
    let investor = Address::generate(&env);
    let result = client.try_place_bid(
        &invoice_id,
        &investor,
        &50,
        &55,
    );
    
    assert!(result.is_err());
    let err = result.unwrap_err().expect("should have error");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn bid_blocked_past_time_limit() {
    let env = Env::default();
    env.mock_all_auths();
    
    let now = 100_000_000u64;
    env.ledger().set_timestamp(now);
    
    let (client, admin, business) = setup(&env);
    
    // due_date is in the future
    let due_date = now + 10_000;
    let invoice_id = create_test_invoice(&env, &client, &business, &admin, due_date);
    
    // Advance time to PAST the due_date
    env.ledger().set_timestamp(due_date + 1);
    
    let investor = Address::generate(&env);
    let result = client.try_place_bid(
        &invoice_id,
        &investor,
        &50,
        &55,
    );
    
    assert!(result.is_err());
    let err = result.unwrap_err().expect("should have error");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}
