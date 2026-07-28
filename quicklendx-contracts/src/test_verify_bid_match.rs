//! Tests for `verify_bid_match` — the precondition check for matching a bid
//! to an invoice before bid acceptance / matching logic.
//!
//! # Context
//!
//! `verify_bid_match` validates four conditions:
//! 1. The bid belongs to the invoice (`bid.invoice_id == invoice.id`).
//! 2. The bid is in `Placed` status.
//! 3. The bid has not expired.
//! 4. The bid amount is positive.
//!
//! # Negative tests
//!
//! Each condition has a corresponding negative test that produces a typed
//! error.  Before this helper existed, the checks were inlined in
//! `load_accept_bid_context`; extracting them into a reusable helper makes
//! the precondition independently testable and auditable.
//!
//! # Threat mitigated
//!
//! Without this explicit precondition check, a caller could attempt to match
//! a bid that belongs to a different invoice, or one that has already expired
//! or been cancelled, leading to state corruption or inconsistent accounting.
//! Each guard returns a distinct typed error so the caller (and any audit
//! monitor) can distinguish between a wrong-invoice call (`Unauthorized`),
//! an expired bid (`InvalidStatus`), and a zero-amount bid (`InvalidAmount`).

#![cfg(test)]

use crate::bid::{verify_bid_match, Bid, BidStatus};
use crate::errors::QuickLendXError;
use crate::types::{Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Helpers
// ============================================================================

fn make_invoice(env: &Env, id: BytesN<32>) -> Invoice {
    Invoice {
        id,
        business: Address::generate(env),
        amount: 1_000i128,
        currency: Address::generate(env),
        due_date: env.ledger().timestamp() + 86_400,
        description: String::from_str(env, "test invoice"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(env),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        status: InvoiceStatus::Verified,
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(env),
        dispute_status: DisputeStatus::None,
        dispute: Dispute {
            created_by: Address::generate(env),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: Address::generate(env),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(env),
        created_at: env.ledger().timestamp(),
        origination_fee_bps: None,
    }
}

fn make_bid(env: &Env, invoice_id: &BytesN<32>, investor: &Address, id_suffix: u8) -> Bid {
    let mut bid_id_bytes = [0u8; 32];
    bid_id_bytes[0] = 0xB1;
    bid_id_bytes[30] = id_suffix;
    bid_id_bytes[31] = id_suffix;

    Bid {
        bid_id: BytesN::from_array(env, &bid_id_bytes),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1_000i128,
        expected_return: 1_200i128,
        timestamp: env.ledger().timestamp(),
        status: BidStatus::Placed,
        expiration_timestamp: env.ledger().timestamp() + 604_800,
    }
}

// ============================================================================
// Match (happy path)
// ============================================================================

/// All four conditions hold — the helper must return Ok(()).
#[test]
fn verify_bid_match_succeeds_for_valid_match() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let bid = make_bid(&env, &invoice_id, &investor, 1);

    verify_bid_match(&env, &bid, &invoice)
        .expect("valid bid-invoice pair must pass");
}

// ============================================================================
// Mismatch — wrong invoice
// ============================================================================

/// NEGATIVE TEST — bid references a different invoice.
#[test]
fn verify_bid_match_rejects_wrong_invoice() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let wrong_id: BytesN<32> = BytesN::from_array(&env, &[2; 32]);
    let invoice = make_invoice(&env, invoice_id);
    let investor = Address::generate(&env);
    let bid = make_bid(&env, &wrong_id, &investor, 1);

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("bid referencing different invoice must fail");
    assert_eq!(err, QuickLendXError::Unauthorized);
}

// ============================================================================
// Mismatch — wrong status
// ============================================================================

/// NEGATIVE TEST — bid is Cancelled, not Placed.
#[test]
fn verify_bid_match_rejects_non_placed_status() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.status = BidStatus::Cancelled;

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("cancelled bid must fail");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn verify_bid_match_rejects_accepted_status() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.status = BidStatus::Accepted;

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("accepted bid must fail");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn verify_bid_match_rejects_expired_status() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.status = BidStatus::Expired;

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("expired-status bid must fail");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn verify_bid_match_rejects_withdrawn_status() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.status = BidStatus::Withdrawn;

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("withdrawn bid must fail");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// Mismatch — expired bid
// ============================================================================

/// NEGATIVE TEST — bid's expiration_timestamp is in the past.
#[test]
fn verify_bid_match_rejects_expired_timestamp() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.expiration_timestamp = 999_999; // expired (before current timestamp)

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("expired bid must fail");
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// Mismatch — zero amount
// ============================================================================

/// NEGATIVE TEST — bid amount is zero.
#[test]
fn verify_bid_match_rejects_zero_amount() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.bid_amount = 0;

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("zero-amount bid must fail");
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

/// NEGATIVE TEST — bid amount is negative.
#[test]
fn verify_bid_match_rejects_negative_amount() {
    let env = Env::default();
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1; 32]);
    let invoice = make_invoice(&env, invoice_id.clone());
    let investor = Address::generate(&env);
    let mut bid = make_bid(&env, &invoice_id, &investor, 1);
    bid.bid_amount = -1;

    let err = verify_bid_match(&env, &bid, &invoice)
        .expect_err("negative-amount bid must fail");
    assert_eq!(err, QuickLendXError::InvalidAmount);
}
